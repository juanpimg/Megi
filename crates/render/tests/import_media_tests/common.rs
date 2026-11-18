//! Shared helpers for the `@import`, media-query and showcase-golden tests.

pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::{Arc, Mutex};

pub(crate) use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct RouteResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RouteResponse {
    pub(crate) fn new(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body,
        }
    }

    pub(crate) fn html(body: &str) -> Self {
        Self::new("text/html; charset=utf-8", body.as_bytes().to_vec())
    }

    pub(crate) fn css(body: &str) -> Self {
        Self::new("text/css", body.as_bytes().to_vec())
    }
}

pub(crate) struct TestServer {
    base: String,
    requested: Arc<Mutex<Vec<String>>>,
    expected: usize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn spawn(routes: HashMap<String, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let address = listener.local_addr().expect("address");
        let requested = Arc::new(Mutex::new(Vec::new()));
        let requested_thread = Arc::clone(&requested);
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_path(&mut stream);
                        requested_thread.lock().expect("log").push(path.clone());
                        let response = routes.get(&path).cloned().unwrap_or(RouteResponse {
                            status: 404,
                            headers: Vec::new(),
                            body: Vec::new(),
                        });
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
        self.requested.lock().expect("log").clone()
    }

    pub(crate) fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("server thread");
        }
        assert_eq!(
            self.requests().len(),
            self.expected,
            "expected {} requests, got {:?}",
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

fn read_path(stream: &mut TcpStreamLike) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
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
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|target| target.split('?').next().unwrap_or("/").to_string())
        .unwrap_or_default()
}

type TcpStreamLike = std::net::TcpStream;

fn write_response(stream: &mut std::net::TcpStream, response: &RouteResponse) {
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

pub(crate) fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        fetch_options: megi_net::FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..megi_net::FetchOptions::default()
        },
        ..RenderOptions::default()
    }
}

pub(crate) fn count_colored(
    output: &RenderOutput,
    predicate: impl Fn(&megi_paint::Color) -> bool,
) -> usize {
    output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| predicate(pixel))
        .count()
}

pub(crate) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

pub(crate) fn showcase_url() -> String {
    format!(
        "file://{}",
        fixture_dir().join("showcase/index.html").display()
    )
}
