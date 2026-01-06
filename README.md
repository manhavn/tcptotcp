# SETUP

- Github: [https://github.com/manhavn/tcptotcp](https://github.com/manhavn/tcptotcp)
- Crate: [https://crates.io/crates/tcptotcp](https://crates.io/crates/tcptotcp)

```shell
 cargo add tcptotcp
```

- `Cargo.toml`

```toml
# ...

[dependencies]
#tcptotcp = { git = "https://github.com/manhavn/tcptotcp.git" }
tcptotcp = "0.0.2" # https://crates.io/crates/tcptotcp
```

- `test.rs`

```rust
#[cfg(test)]
mod tests {
    use tcptotcp::connect;
    use crossbeam::channel::unbounded;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_tcp_server() {
        let listener = TcpListener::bind("localhost:9000").unwrap();
        let timeout_alive_sec: u64 = 7_200; // waiting 2 hours { 60s * 60p * 2h = 7200s }

        let (tx, rx) = unbounded::<TcpStream>();
        for stream in listener.incoming() {
            match stream {
                Ok(stream_client) => match rx.try_recv() {
                    Ok(stream_app) => {
                        thread::spawn(|| connect(stream_client, stream_app, timeout_alive_sec));
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
        let timeout_alive_sec: u64 = 7_200; // waiting 2 hours { 60s * 60p * 2h = 7200s }

        connect(stream_server, stream_app, timeout_alive_sec).unwrap();
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
