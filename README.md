# tcptotcp

A tiny, dependency-free TCP bridge that relays bytes **bidirectionally** between two `TcpStream`s.

- ✅ No async runtime
- ✅ No dependencies
- ✅ Blocking I/O using `std::thread`
- ✅ Good fit for “client socket ↔ upstream socket” forwarding

## Install

```bash
cargo add tcptotcp
```

Or in Cargo.toml:

```toml
[dependencies]
tcptotcp = "0.0.5"
```

---

## Quick start (local forwarder)

Accept a local client, connect to an upstream, then bridge both sockets:

```rust
use std::net::{TcpListener, TcpStream};
use std::thread;
use tcptotcp::connect;

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9000")?;
    println!("Listening on 127.0.0.1:9000");

    for incoming in listener.incoming() {
        let client = incoming?;
        let upstream = TcpStream::connect("example.com:80")?;

        // Check every 5 seconds, close if no bridged traffic for 2 hours.
        let rate_check_seconds: u8 = 5;
        let idle_timeout_seconds: u64 = 7_200;

        thread::spawn(move || {
            let _ = connect(client, upstream, rate_check_seconds, idle_timeout_seconds);
        });
    }

    Ok(())
}
```

> Use no_run in docs.rs examples if you paste this into Rustdoc (network is not available during doctests).
---

## Traffic-based idle timeout (not TCP keepalive)

`connect()` does not send keepalive probes.
It treats the connection as “alive” only when data is relayed.
Parameters:

- `rate_check_seconds`: how often the calling thread checks for traffic (seconds)
- `keep_alive_delay_time_seconds`: maximum allowed idle time without traffic (seconds)

When the idle timeout triggers, both streams are shut down and connect() returns.

---

## Test

- `test.rs`

```rust
#[cfg(test)]
mod tests {
    use crossbeam::channel::unbounded;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;
    use tcptotcp::connect;

    #[test]
    fn test_tcp_server() {
        let listener = TcpListener::bind("localhost:9000").unwrap();

        let (tx, rx) = unbounded::<TcpStream>();
        for stream in listener.incoming() {
            match stream {
                Ok(stream_client) => match rx.try_recv() {
                    Ok(stream_app) => {
                        thread::spawn(|| connect(stream_client, stream_app, 5, 7_200));
                    }
                    _ => {
                        tx.send(stream_client).ok();
                    }
                },
                Err(_) => {}
            }
        }
    }

    #[test]
    fn test_tcp_client() {
        let stream_server = TcpStream::connect("localhost:9000").unwrap();
        let stream_app = TcpStream::connect("google.com:80").unwrap();
        let rate_check_seconds: u8 = 5;
        let keep_alive_delay_time_seconds: u64 = 7_200; // waiting 2 hours { 60s * 60p * 2h = 7200s }

        connect(
            stream_server,
            stream_app,
            rate_check_seconds,
            keep_alive_delay_time_seconds,
        )
            .unwrap();
    }

    #[test]
    fn open_web_link() {
        thread::sleep(Duration::from_secs(1));
        let url = "http://localhost:9000";

        #[cfg(target_os = "linux")]
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .unwrap();

        #[cfg(target_os = "macos")]
        std::process::Command::new("open").arg(url).spawn().unwrap();

        #[cfg(target_os = "windows")]
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .unwrap();
    }
}
```
