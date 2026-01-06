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
    let rate_check = Duration::from_millis(rate_check_seconds as u64);
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
