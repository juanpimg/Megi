//! Shared helpers for the end-to-end JavaScript render tests.

pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::Arc;
pub(crate) use std::time::{Duration, Instant};

pub(crate) use megi_render::{
    ConsoleMessage, RenderOptions, ScriptBudget, ScriptingLimits, ScriptingMode, Viewport,
    render_url,
};

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub(crate) fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

pub(crate) fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

pub(crate) fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(800, 600).expect("viewport"),
        ..RenderOptions::default()
    }
}

pub(crate) fn count_color(pixmap: &megi_paint::Pixmap, color: megi_css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

pub(crate) const RED: megi_css::Color = megi_css::Color::new(0xFF, 0x00, 0x00, 0xFF);
pub(crate) const GREEN: megi_css::Color = megi_css::Color::new(0x00, 0xFF, 0x00, 0xFF);
pub(crate) const BLUE: megi_css::Color = megi_css::Color::new(0x00, 0x00, 0xFF, 0xFF);
pub(crate) const MAGENTA: megi_css::Color = megi_css::Color::new(0xFF, 0x00, 0xFF, 0xFF);

pub(crate) fn render_fixture(name: &str, options: &RenderOptions) -> megi_render::RenderOutput {
    let url = file_url(&fixture_dir().join(name));
    render_url(&url, options).unwrap_or_else(|error| panic!("render {name}: {error}"))
}

// ---------------------------------------------------------------------------
// Local HTTP server for external-script tests
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct RouteResponse {
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct TestServer {
    base: String,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn spawn(routes: HashMap<&'static str, RouteResponse>, expected: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let address = listener.local_addr().expect("local addr");
        let routes = Arc::new(routes);
        let handle = std::thread::spawn(move || {
            let mut served = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while served < expected && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let path = read_request_path(&mut stream).unwrap_or_default();
                        let response =
                            routes
                                .get(path.as_str())
                                .cloned()
                                .unwrap_or_else(|| RouteResponse {
                                    content_type: "text/plain",
                                    body: b"not found".to_vec(),
                                });
                        let _ = stream.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                response.content_type,
                                response.body.len()
                            )
                            .as_bytes(),
                        );
                        let _ = stream.write_all(&response.body);
                        let _ = stream.flush();
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
            handle: Some(handle),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> std::io::Result<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    Ok(request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string())
}
