//! Offline integration tests for the Phase-5 browser features: cookie `SameSite` + reduced
//! PSL, `Referrer-Policy`, byte ranges (`206`) and the improved `Cache-Control` policy.
//!
//! Every networked test binds its own `127.0.0.1:0` listener and serves hand-written
//! responses; no external network, DNS beyond loopback or long sleeps.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, UNIX_EPOCH};

use megi_net::{
    ByteRange, ContentRange, CookieJar, FetchOptions, RangeError, ReferrerConfig, ReferrerPolicy,
    RequestSite, ResourceLoader, SameSite, SetCookieError, Url, fetch, is_public_suffix,
    parse_cache_policy, parse_set_cookie, registrable_domain, same_origin, same_site, site_for_url,
};

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

fn url(raw: &str) -> Url {
    Url::parse(raw).expect("test URL parses")
}

/// Tiny one-shot HTTP server: writes `responses[i]` on connection `i`, returns requests.
fn spawn_server(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut request = Vec::new();
            let mut buffer = [0u8; 2048];
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

fn request_text(request: &[u8]) -> String {
    String::from_utf8_lossy(request).into_owned()
}

fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (header, value) = line.split_once(':')?;
        header.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

mod cache;
mod cookies;
mod psl_sites;
mod range;
mod referrer;
