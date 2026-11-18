//! Shared helpers for the advanced-`net` end-to-end tests.

pub(crate) use std::collections::HashMap;
pub(crate) use std::sync::{Arc, Mutex};

pub(crate) use megi_render::{RenderOptions, Viewport, render_url};

use megi_paint::Color;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::thread::JoinHandle;

pub(crate) fn options(width: u32, height: u32) -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(width, height).expect("viewport"),
        ..RenderOptions::default()
    }
}

// ---------------------------------------------------------------------------
// Test-only gzip/zlib encoders (stored DEFLATE blocks; no compression).
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    loop {
        let chunk = (data.len() - offset).min(65_535);
        let last = offset + chunk >= data.len();
        out.push(u8::from(last));
        out.extend_from_slice(&(chunk as u16).to_le_bytes());
        out.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + chunk]);
        offset += chunk;
        if last {
            break;
        }
    }
    out
}

pub(crate) fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

pub(crate) fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

// ---------------------------------------------------------------------------
// Minimal routed loopback HTTP server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct Route {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

pub(crate) fn route(status: u16, headers: Vec<(&str, &str)>, body: Vec<u8>) -> Route {
    Route {
        status,
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        body,
    }
}

pub(crate) fn html(body: &str) -> Route {
    route(
        200,
        vec![("Content-Type", "text/html; charset=utf-8")],
        body.as_bytes().to_vec(),
    )
}

/// Serves `routes` keyed by request path for `connections` connections and returns the raw
/// request heads captured for each connection.
///
/// Accepts with a deadline so a test that issues fewer requests than expected cannot hang
/// forever; per-connection reads are also time-bounded.
pub(crate) fn spawn_router(
    routes: HashMap<String, Route>,
    connections: usize,
) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("set listener non-blocking");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut requests = Vec::new();
        for _ in 0..connections {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return requests;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => return requests,
                }
            };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
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
            let text = String::from_utf8_lossy(&request);
            let path = text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            requests.push(text.into_owned());
            let response = routes.get(&path).cloned().unwrap_or_else(|| {
                route(
                    404,
                    vec![("Content-Type", "text/plain")],
                    b"not found".to_vec(),
                )
            });
            let reason = match response.status {
                200 => "OK",
                302 => "Found",
                _ => "Error",
            };
            let mut head = format!("HTTP/1.1 {} {reason}\r\n", response.status);
            for (name, value) in &response.headers {
                head.push_str(name);
                head.push_str(": ");
                head.push_str(value);
                head.push_str("\r\n");
            }
            head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
            let mut bytes = head.into_bytes();
            bytes.extend_from_slice(&response.body);
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        requests
    });
    (format!("http://{address}"), handle)
}

pub(crate) fn header_value(request: &str, name: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (header_name, value) = line.split_once(':')?;
        header_name
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

pub(crate) const RED: Color = Color::new(0xFF, 0x00, 0x00, 0xFF);
pub(crate) const BLUE: Color = Color::new(0x00, 0x00, 0xFF, 0xFF);
