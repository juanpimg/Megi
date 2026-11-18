//! Offline integration tests for the Phase-5 advanced networking features: cookies, CORS
//! and gzip/deflate response decoding.
//!
//! Every test binds its own `127.0.0.1:0` listener and serves hand-written responses; no
//! external network, no DNS beyond loopback and no long sleeps.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::compression::crc32_ieee;
use megi_net::{
    AcceptEncoding, CompressionLimits, CookieJar, FetchError, FetchOptions, Url, fetch, preflight,
};

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

/// A tiny one-shot HTTP server: writes `responses[i]` on connection `i` and returns the
/// raw request head received for every connection once the thread is joined.
fn spawn_server(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
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
            requests.push(request);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        requests
    });
    (format!("http://{address}"), handle)
}

fn base_url(base: &str) -> Url {
    Url::parse(base).expect("test server base URL parses")
}

fn response(status_line: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status_line}\r\n");
    for (name, value) in headers {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn ok(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    response("200 OK", headers, body)
}

fn request_text(request: &[u8]) -> String {
    String::from_utf8_lossy(request).into_owned()
}

// ---------------------------------------------------------------------------
// Test-only gzip/zlib encoders (stored DEFLATE blocks; no compression).
// ---------------------------------------------------------------------------

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

fn gzip_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xFF];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&crc32_ieee(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(&deflate_stored(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn bomb_gzip() -> Vec<u8> {
    let hex = include_str!("../fixtures/gzip_bomb_zeros_1mib.hex");
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex fixture"))
        .collect()
}

fn jar() -> Arc<Mutex<CookieJar>> {
    Arc::new(Mutex::new(CookieJar::new()))
}

mod compression;
mod cookies;
mod cors;
