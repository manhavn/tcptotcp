//! # tcptotcp
//!
//! A tiny, dependency-free TCP bridge that **relays bytes bidirectionally**
//! between two [`std::net::TcpStream`] connections.
//!
//! This crate is useful when you already have **two established TCP streams**
//! (e.g. *client socket* and *upstream socket*) and you want to forward traffic
//! between them with a simple, blocking implementation.
//!
//! ## What it does
//!
//! - Spawns **two threads**:
//!   - Thread A: reads from stream1 and writes to stream2
//!   - Thread B: reads from stream2 and writes to stream1
//! - Tracks **activity (traffic)** in both directions.
//! - Runs a periodic check loop on the calling thread:
//!   - If both directions had traffic recently, the idle counter resets.
//!   - If idle persists beyond the configured threshold, both streams are shut down
//!     and `connect()` returns.
//!
//! ## Important notes
//!
//! - This is **not OS-level TCP keepalive** (it does not send keepalive probes).
//!   “Alive” here means **traffic passed through** the bridge.
//! - `connect()` is **blocking**: it returns when either side closes, an I/O error occurs,
//!   or the traffic-based idle timeout triggers.
//! - The implementation uses blocking I/O + `std::thread`.
//!
//! ## Parameters
//!
//! - `rate_check_seconds`: how often the calling thread checks for traffic (seconds).
//!   Values `< 1` are clamped to `1`.
//! - `keep_alive_delay_time_seconds`: max allowed idle time without traffic (seconds).
//!   Values `< 2` are clamped to `2`.
//!
//! Internally the idle limit is roughly:
//! `max_delay = keep_alive_delay_time_seconds / rate_check_seconds` (floor).
//!
//! ## Example: accept local clients and forward to an upstream
//!
//! ```rust,no_run
//! use std::net::{TcpListener, TcpStream};
//! use std::thread;
//! use tcptotcp::connect;
//!
//! fn main() -> std::io::Result<()> {
//!     let listener = TcpListener::bind("127.0.0.1:9000")?;
//!     println!("Listening on 127.0.0.1:9000");
//!
//!     for incoming in listener.incoming() {
//!         let client = incoming?;
//!         let upstream = TcpStream::connect("example.com:80")?;
//!
//!         // Check every 5s, close if there is no bridged traffic for 2 hours.
//!         let rate_check_seconds: u8 = 5;
//!         let idle_timeout_seconds: u64 = 7_200;
//!
//!         thread::spawn(move || {
//!             let _ = connect(client, upstream, rate_check_seconds, idle_timeout_seconds);
//!         });
//!     }
//!
//!     Ok(())
//! }
//! ```

use std::io::{Read, Result, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

fn stream(
    closed: Arc<AtomicBool>,
    mut reader: TcpStream,
    mut writer: TcpStream,
    ping: Arc<AtomicBool>,
) {
    let mut buf: Vec<u8> = vec![0u8; 16 * 1024];
    loop {
        if closed.load(Ordering::Relaxed) {
            break;
        } else {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if writer.write(&buf[..n]).is_err() {
                        break;
                    }
                    ping.store(true, Ordering::Release);
                }
                Err(_) => break,
            }
        }
    }
    closed.store(true, Ordering::Release);
    writer.shutdown(Shutdown::Both).ok();
    reader.shutdown(Shutdown::Both).ok();
}

fn config_stream(
    closed: &Arc<AtomicBool>,
    stream1: &TcpStream,
    stream2: &TcpStream,
    ping1: &Arc<AtomicBool>,
) -> Result<()> {
    let closed = Arc::clone(&closed);
    let ping1 = Arc::clone(&ping1);
    let stream1 = stream1.try_clone()?;
    let stream2 = stream2.try_clone()?;
    thread::spawn(move || stream(closed, stream1, stream2, ping1));
    Ok(())
}

/// Bridge (relay) bytes **bidirectionally** between two `TcpStream`s.
///
/// ## Behavior
/// - Spawns two relay threads (stream1→stream2 and stream2→stream1).
/// - The calling thread periodically checks whether **both directions** have seen traffic.
/// - If traffic is absent for too long, both sockets are shut down and the function returns.
///
/// ## Traffic-based keep-alive (idle timeout)
/// This function does **not** send any ping/keepalive packets.
/// It only considers the connection “active” when data is successfully relayed.
///
/// If you need true TCP keepalive probes, configure them on the sockets
/// before calling this function.
///
/// ## Parameters
/// - `rate_check_seconds`: traffic check interval (seconds). `< 1` is clamped to `1`.
/// - `keep_alive_delay_time_seconds`: max idle time allowed (seconds). `< 2` is clamped to `2`.
///
/// ## Blocking
/// This function is **blocking** and returns when:
/// - either side closes,
/// - an I/O error happens,
/// - or the traffic-based idle timeout triggers.
///
/// ## Example
/// ```rust,no_run
/// use std::net::TcpStream;
/// use tcptotcp::connect;
///
/// fn main() -> std::io::Result<()> {
///     let a = TcpStream::connect("127.0.0.1:9000")?;
///     let b = TcpStream::connect("example.com:80")?;
///     connect(a, b, 5, 7_200)?;
///     Ok(())
/// }
/// ```
pub fn connect(
    stream1: TcpStream,
    stream2: TcpStream,
    mut rate_check_seconds: u8,
    mut keep_alive_delay_time_seconds: u64,
) -> Result<()> {
    let closed = Arc::new(AtomicBool::new(false));
    let ping1 = Arc::new(AtomicBool::new(false));
    let ping2 = Arc::new(AtomicBool::new(false));

    config_stream(&closed, &stream1, &stream2, &ping1)?;
    config_stream(&closed, &stream2, &stream1, &ping2)?;

    if rate_check_seconds < 1 {
        rate_check_seconds = 1
    }
    if keep_alive_delay_time_seconds < 2 {
        keep_alive_delay_time_seconds = 2
    }

    let mut delay: u64 = 0;
    let max_delay = keep_alive_delay_time_seconds / rate_check_seconds as u64;
    let rate_check = Duration::from_secs(rate_check_seconds as u64);
    loop {
        if ping1.load(Ordering::Acquire) && ping2.load(Ordering::Acquire) {
            ping1.store(false, Ordering::Release);
            ping2.store(false, Ordering::Release);
            delay = 0; // reset delay count
        } else {
            if delay > max_delay {
                closed.store(true, Ordering::Release);
                stream1.shutdown(Shutdown::Both).ok();
                stream2.shutdown(Shutdown::Both).ok();
                break;
            }
            delay += 1;
            thread::sleep(rate_check);
            if closed.load(Ordering::Acquire) {
                break;
            }
        }
    }
    Ok(())
}
