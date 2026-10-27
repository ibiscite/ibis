#![allow(clippy::unwrap_used)]
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
pub struct Server {
    pub port: u16,
    pub instance: Arc<AtomicU64>,
    pub pause: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let instance = Arc::new(AtomicU64::new(1));
        let pause = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (id, wait, seen, stopping) = (instance.clone(), pause.clone(), paused.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = vec![];
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let inventory = String::from_utf8(request).unwrap().contains("format=versions");
                if !inventory && wait.load(Ordering::Relaxed) {
                    seen.store(true, Ordering::SeqCst);
                    while wait.load(Ordering::Relaxed) && !stopping.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                let body = if inventory {
                    json!({"SYNTH001":1})
                } else {
                    json!([{"key":"SYNTH001","version":1,"data":{"key":"SYNTH001","version":1,"itemType":"book","title":"Invented atlas","date":"2026","citationKey":"Synthetic2026","creators":[],"dateModified":"2026-10-26T12:00:00Z"}}])
                };
                let body = serde_json::to_vec(&body).unwrap();
                let id = id.load(Ordering::Relaxed);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nZotero-API-Version: 3\r\nX-Zotero-Version: 10.0.6\r\nZotero-Server-ID: invented-{id}\r\nLast-Modified-Version: 1\r\nTotal-Results: 1\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            port,
            instance,
            pause,
            paused,
            stop,
            worker: Some(worker),
        }
    }
    pub fn wait_for_payload(&self) {
        let start = Instant::now();
        while !self.paused.load(Ordering::SeqCst) {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
