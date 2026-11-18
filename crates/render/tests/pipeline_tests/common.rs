//! Shared helpers for the offline `render`-pipeline integration tests.

pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::Arc;

pub(crate) use megi_render::{RenderError, RenderOptions, RenderOutput, Viewport, render_url};

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Local HTTP test server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct RouteResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl RouteResponse {
    pub(crate) fn html(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![(
                "Content-Type".to_string(),
                "text/html; charset=utf-8".to_string(),
            )],
            body: body.as_bytes().to_vec(),
        }
    }

    pub(crate) fn css(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/css".to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: b"not found".to_vec(),
        }
    }

    pub(crate) fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".to_string(), location.to_string())],
            body: Vec::new(),
        }
    }
}

/// A one-shot loopback HTTP server serving `routes` for exactly `expected` connections.
pub(crate) struct TestServer {
    base: String,
    requested: Arc<std::sync::Mutex<Vec<String>>>,
    expected: usize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn spawn(routes: HashMap<String, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let address = listener.local_addr().expect("listener address");
        let requested = Arc::new(std::sync::Mutex::new(Vec::new()));
        let requested_thread = Arc::clone(&requested);
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_request_path(&mut stream).unwrap_or_default();
                        requested_thread
                            .lock()
                            .expect("request log lock")
                            .push(path.clone());
                        let response = routes
                            .get(&path)
                            .cloned()
                            .unwrap_or(RouteResponse::status(404));
                        write_response(&mut stream, &response);
                        served += 1;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{address}"),
            requested,
            expected,
            handle: Some(handle),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub(crate) fn requests(&self) -> Vec<String> {
        self.requested.lock().expect("request log lock").clone()
    }

    pub(crate) fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("server thread");
        }
        assert_eq!(
            self.requests().len(),
            self.expected,
            "server expected {} connections, served {:?}",
            self.expected,
            self.requests()
        );
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let head = String::from_utf8_lossy(&request);
    head.lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .map(|target| target.split('?').next().unwrap_or("/").to_string())
}

fn write_response(stream: &mut TcpStream, response: &RouteResponse) {
    let mut head = format!("HTTP/1.1 {} X\r\n", response.status);
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
}

/// Port with no listener (connection refused) for deterministic fetch errors.
pub(crate) fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind spare listener");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    port
}

pub(crate) fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("valid viewport"),
        fetch_options: megi_net::FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..megi_net::FetchOptions::default()
        },
        ..RenderOptions::default()
    }
}

pub(crate) fn pixel_is(pixmap: &megi_paint::Pixmap, x: u32, y: u32, r: u8, g: u8, b: u8) -> bool {
    let pixel = pixmap.pixel(x, y);
    pixel.r == r && pixel.g == g && pixel.b == b
}

pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

pub(crate) fn assert_render_output_consistency(output: &RenderOutput) {
    assert_eq!(
        output.dimensions(),
        (output.pixmap.width, output.pixmap.height)
    );
    assert!(!output.png.is_empty());
    assert_eq!(&output.png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(output.display_list.items.len(), output.stats.display_items);
    assert!(output.stats.limit_violations.is_empty());
    assert!(output.stats.timings.total >= output.stats.timings.fetch);
    assert_eq!(output.stats.charset, "utf-8");
}
