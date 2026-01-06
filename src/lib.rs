use std::io::{Read, Result, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

fn stream(
    writer_closed: Arc<AtomicBool>,
    reader_closed: Arc<AtomicBool>,
    keep_alive_ping: Arc<AtomicBool>,
    mut stream_read: TcpStream,
    mut stream_write: TcpStream,
) {
    let mut buf: Vec<u8> = vec![0u8; 16 * 1024];
    loop {
        if reader_closed.load(Ordering::Relaxed) {
            break;
        } else {
            match stream_read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if stream_write.write(&buf[..n]).is_err() {
                        break;
                    }
                    keep_alive_ping.store(true, Ordering::Release);
                }
                Err(_) => break,
            }
        }
    }
    writer_closed.store(true, Ordering::Release);
    stream_write.flush().ok();
    stream_write.shutdown(Shutdown::Both).ok();
}

pub fn connect(
    stream_client: TcpStream,
    stream_app: TcpStream,
    timeout_alive_sec: u64,
) -> Result<()> {
    let clone_stream_client = stream_client.try_clone()?;
    let clone_stream_app = stream_app.try_clone()?;
    let clone_stream_client_closed = stream_client.try_clone()?;
    let clone_stream_app_closed = stream_app.try_clone()?;

    let worker1_closed = Arc::new(AtomicBool::new(false));
    let worker2_closed = Arc::new(AtomicBool::new(false));
    let worker1_closed_clone1 = Arc::clone(&worker1_closed);
    let worker1_closed_clone2 = Arc::clone(&worker1_closed);
    let worker1_closed_clone3 = Arc::clone(&worker1_closed);
    let worker2_closed_clone1 = Arc::clone(&worker2_closed);
    let worker2_closed_clone2 = Arc::clone(&worker2_closed);
    let worker2_closed_clone3 = Arc::clone(&worker2_closed);

    let keep_alive1 = Arc::new(AtomicBool::new(false));
    let keep_alive2 = Arc::new(AtomicBool::new(false));
    let ping_keep_alive_rw1 = Arc::clone(&keep_alive1);
    let ping_keep_alive_rw2 = Arc::clone(&keep_alive2);
    let ping_keep_alive_wo1 = Arc::clone(&keep_alive1);
    let ping_keep_alive_wo2 = Arc::clone(&keep_alive2);

    thread::spawn(move || {
        let timeout: u64 = 5; // timeout rate 5s
        let count_max_waiting = timeout_alive_sec / timeout;
        thread::sleep(Duration::from_secs(timeout)); // waiting first connection
        let mut count_waiting: u64 = 0;
        loop {
            let ping1 = ping_keep_alive_rw1.load(Ordering::Acquire);
            let ping2 = ping_keep_alive_rw2.load(Ordering::Acquire);
            if ping1 && ping2 {
                ping_keep_alive_rw1.store(false, Ordering::Release);
                ping_keep_alive_rw2.store(false, Ordering::Release);
                count_waiting = 0; // reset add count
            } else {
                count_waiting += 1; // add count
                // waiting 2 hours
                if count_waiting >= count_max_waiting {
                    worker1_closed_clone3.store(true, Ordering::Release);
                    worker2_closed_clone3.store(true, Ordering::Release);
                    clone_stream_client_closed.shutdown(Shutdown::Both).ok();
                    clone_stream_app_closed.shutdown(Shutdown::Both).ok();
                    break;
                }
                thread::sleep(Duration::from_secs(timeout));

                let worker1_is_closed = worker1_closed_clone3.load(Ordering::Acquire);
                let worker2_is_closed = worker2_closed_clone3.load(Ordering::Acquire);
                if worker1_is_closed || worker2_is_closed {
                    break;
                }
            }
        }
    });
    thread::spawn(move || {
        stream(
            worker1_closed_clone1,
            worker2_closed_clone1,
            ping_keep_alive_wo1,
            clone_stream_app,
            clone_stream_client,
        )
    });
    stream(
        worker1_closed_clone2,
        worker2_closed_clone2,
        ping_keep_alive_wo2,
        stream_client,
        stream_app,
    );
    Ok(())
}
