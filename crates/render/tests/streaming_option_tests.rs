//! R185: `RenderOptions::stream_responses` routes the main `http(s)` document through
//! `net::fetch_streaming`. The buffered and streaming paths must render byte-identically
//! (#1390): the old test used a `file://` fixture, which never reaches the streaming path,
//! so it certified a no-op option.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use render::{RenderOptions, Viewport, render_url};

const HTML: &str = "<!doctype html><html><head><style>body{background:rgb(20,120,220)}</style>\
</head><body><h1>Streaming test</h1></body></html>";

/// Loopback server serving `HTML` to any request until stopped. Tracks how many bodies it
/// wrote so the two renders (buffered + streaming) each get a fresh, independent fetch.
struct Server {
    base: String,
    served: Arc<AtomicUsize>,
    stop: Arc<AtomicUsize>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        listener.set_nonblocking(true).expect("set nonblocking");
        let address = listener.local_addr().expect("address");
        let served = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicUsize::new(0));
        let served_thread = Arc::clone(&served);
        let stop_thread = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while stop_thread.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Read the request headers so the peer sees a complete exchange.
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        let body = HTML.as_bytes();
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                             Content-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                        let _ = stream.flush();
                        served_thread.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            served,
            stop,
            handle: Some(handle),
        }
    }

    fn options(&self, stream_responses: bool) -> RenderOptions {
        RenderOptions {
            viewport: Viewport::new(200, 100).expect("viewport"),
            stream_responses,
            ..RenderOptions::default()
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(1, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[test]
fn stream_responses_path_is_byte_identical_to_buffered() {
    let server = Server::spawn();
    let url = format!("{}/index.html", server.base);

    let buffered = render_url(&url, &server.options(false)).expect("buffered render");
    assert_eq!(buffered.stats.status, Some(200));
    let streaming = render_url(&url, &server.options(true)).expect("streaming render");
    assert_eq!(streaming.stats.status, Some(200));

    // Each render performed a real HTTP fetch; both paths must produce identical bytes.
    assert!(
        server.served.load(Ordering::Relaxed) >= 2,
        "both paths must fetch the document"
    );
    assert_eq!(buffered.png, streaming.png, "PNG bytes must be identical");
    assert_eq!(
        buffered.pixmap.pixels, streaming.pixmap.pixels,
        "pixels must be identical"
    );
    assert_eq!(
        buffered.stats.document_bytes, streaming.stats.document_bytes,
        "the streamed body must accumulate to the same length as the buffered body"
    );
}
