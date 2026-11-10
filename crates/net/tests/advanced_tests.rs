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

use net::compression::crc32_ieee;
use net::{
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
    let hex = include_str!("fixtures/gzip_bomb_zeros_1mib.hex");
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex fixture"))
        .collect()
}

fn jar() -> Arc<Mutex<CookieJar>> {
    Arc::new(Mutex::new(CookieJar::new()))
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

#[test]
fn cookie_is_stored_and_sent_on_the_next_request() {
    let first = ok(&[("Set-Cookie", "session=abc123; Path=/")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let first_response =
        fetch(&base_url(&base).join("/a").unwrap(), &options).expect("first fetch");
    assert_eq!(first_response.body, b"one");
    let second_response =
        fetch(&base_url(&base).join("/b").unwrap(), &options).expect("second fetch");
    assert_eq!(second_response.body, b"two");

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        !request_text(&requests[0]).contains("Cookie:"),
        "first request must not send a cookie"
    );
    assert!(
        request_text(&requests[1]).contains("Cookie: session=abc123"),
        "second request missing stored cookie: {}",
        request_text(&requests[1])
    );
}

#[test]
fn no_cookie_jar_means_no_cookie_header() {
    let first = ok(&[("Set-Cookie", "a=1")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = options();
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("first fetch");
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("second fetch");
    let requests = server.join().expect("server thread");
    assert!(!request_text(&requests[1]).contains("Cookie:"));
}

#[test]
fn secure_cookie_is_not_sent_over_http() {
    let first = ok(&[("Set-Cookie", "s=secret; Secure")], b"one");
    let second = ok(&[], b"two");
    let (base, server) = spawn_server(vec![first, second]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("first fetch");
    fetch(&base_url(&base).join("/").unwrap(), &options).expect("second fetch");
    let requests = server.join().expect("server thread");
    assert!(
        !request_text(&requests[1]).contains("Cookie:"),
        "Secure cookie leaked over http"
    );
}

#[test]
fn max_age_zero_deletes_cookie() {
    let set = ok(&[("Set-Cookie", "a=1")], b"one");
    let delete = ok(&[("Set-Cookie", "a=; Max-Age=0")], b"two");
    let final_response = ok(&[], b"three");
    let (base, server) = spawn_server(vec![set, delete, final_response]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let url = base_url(&base);
    fetch(&url, &options).expect("set");
    fetch(&url, &options).expect("delete");
    fetch(&url, &options).expect("after delete");
    let requests = server.join().expect("server thread");
    assert!(request_text(&requests[1]).contains("Cookie: a=1"));
    assert!(
        !request_text(&requests[2]).contains("Cookie:"),
        "deleted cookie still sent: {}",
        request_text(&requests[2])
    );
}

#[test]
fn path_cookie_only_sent_for_matching_paths() {
    let set = ok(&[("Set-Cookie", "p=1; Path=/shop")], b"one");
    let matching = ok(&[], b"two");
    let other = ok(&[], b"three");
    let (base, server) = spawn_server(vec![set, matching, other]);
    let options = FetchOptions {
        cookie_jar: Some(jar()),
        ..options()
    };
    let url = base_url(&base);
    fetch(&url.join("/shop").unwrap(), &options).expect("set");
    fetch(&url.join("/shop/cart").unwrap(), &options).expect("matching");
    fetch(&url.join("/blog").unwrap(), &options).expect("other");
    let requests = server.join().expect("server thread");
    assert!(request_text(&requests[1]).contains("Cookie: p=1"));
    assert!(!request_text(&requests[2]).contains("Cookie:"));
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

#[test]
fn gzip_is_negotiated_decoded_and_header_preserved() {
    let html = b"<!DOCTYPE html><html><head><title>gzip</title></head><body>hello</body></html>";
    let first = ok(&[("Content-Encoding", "gzip")], &gzip_stored(html));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/page").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, html);
    assert_eq!(response.header("content-encoding"), Some("gzip"));
    let requests = server.join().expect("server thread");
    assert!(
        request_text(&requests[0]).contains("Accept-Encoding: gzip"),
        "missing Accept-Encoding: {}",
        request_text(&requests[0])
    );
}

#[test]
fn deflate_zlib_body_is_decoded() {
    let css = b"body { color: red; } /* deflate */";
    let first = ok(&[("Content-Encoding", "deflate")], &zlib_stored(css));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/style.css").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, css);
    let _ = server.join();
}

#[test]
fn deflate_raw_body_is_decoded() {
    let data = b"raw deflate payload";
    let first = ok(&[("Content-Encoding", "deflate")], &deflate_stored(data));
    let (base, server) = spawn_server(vec![first]);
    let response = fetch(&base_url(&base).join("/").unwrap(), &options()).expect("fetch");
    assert_eq!(response.body, data);
    let _ = server.join();
}

#[test]
fn identity_mode_sends_no_accept_encoding_and_leaves_body_encoded() {
    let encoded = gzip_stored(b"compressed body");
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        accept_encoding: AcceptEncoding::Identity,
        ..options()
    };
    let response = fetch(&base_url(&base).join("/").unwrap(), &options).expect("fetch");
    assert_eq!(response.body, encoded, "identity mode must not decode");
    let requests = server.join().expect("server thread");
    assert!(
        !request_text(&requests[0]).contains("Accept-Encoding:"),
        "identity mode must not advertise encodings: {}",
        request_text(&requests[0])
    );
}

#[test]
fn corrupt_gzip_crc_is_a_typed_decompression_error() {
    let mut encoded = gzip_stored(b"payload");
    let crc_offset = encoded.len() - 8;
    encoded[crc_offset] ^= 0xFF;
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::Decompression(ref reason) if reason.contains("CRC-32")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn corrupt_gzip_isize_is_a_typed_decompression_error() {
    let mut encoded = gzip_stored(b"payload");
    let isize_offset = encoded.len() - 2;
    encoded[isize_offset] ^= 0x01;
    let first = ok(&[("Content-Encoding", "gzip")], &encoded);
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::Decompression(ref reason) if reason.contains("ISIZE")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn unsupported_content_encoding_is_rejected() {
    let first = ok(&[("Content-Encoding", "br")], b"brotli bytes");
    let (base, server) = spawn_server(vec![first]);
    let error = fetch(&base_url(&base).join("/").unwrap(), &options()).unwrap_err();
    assert!(
        matches!(error, FetchError::ContentEncoding(ref reason) if reason.contains("br")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn compressed_body_larger_than_max_response_bytes_after_decode_is_too_large() {
    let html = vec![b'x'; 4096];
    let first = ok(&[("Content-Encoding", "gzip")], &gzip_stored(&html));
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        max_response_bytes: 1024,
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(matches!(error, FetchError::ResponseTooLarge), "{error}");
    let _ = server.join();
}

#[test]
fn decompression_bomb_hits_ratio_cap_with_typed_error() {
    let first = ok(&[("Content-Encoding", "gzip")], &bomb_gzip());
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        compression_limits: CompressionLimits::new(64 * 1024 * 1024, 100),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::DecompressionLimit { max_ratio: 100, .. }),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn decompression_bomb_hits_absolute_cap_with_typed_error() {
    let first = ok(&[("Content-Encoding", "gzip")], &bomb_gzip());
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        compression_limits: CompressionLimits::new(64 * 1024, 1000),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/").unwrap(), &options).unwrap_err();
    assert!(
        matches!(
            error,
            FetchError::DecompressionLimit {
                max_output: 65536,
                ..
            }
        ),
        "{error}"
    );
    let _ = server.join();
}

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

const APP_ORIGIN: &str = "https://app.example";

#[test]
fn cross_origin_with_exact_acao_passes_and_sends_origin() {
    let first = ok(&[("Access-Control-Allow-Origin", APP_ORIGIN)], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let response = fetch(&base_url(&base).join("/api").unwrap(), &options).expect("CORS allowed");
    assert_eq!(response.body, b"data");
    let requests = server.join().expect("server thread");
    assert!(
        request_text(&requests[0]).contains(&format!("Origin: {APP_ORIGIN}")),
        "missing Origin header: {}",
        request_text(&requests[0])
    );
}

#[test]
fn cross_origin_with_wildcard_passes_without_credentials() {
    let first = ok(&[("Access-Control-Allow-Origin", "*")], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn cross_origin_without_acao_is_a_cors_error() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("Allow-Origin")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn cross_origin_with_wrong_acao_is_a_cors_error() {
    let first = ok(
        &[("Access-Control-Allow-Origin", "https://other.example")],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("not allowed")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn credentialed_wildcard_is_a_cors_error() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Credentials", "true"),
        ],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        cookie_jar: Some(jar()),
        ..options()
    };
    let error = fetch(&base_url(&base).join("/api").unwrap(), &options).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("credentials")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn credentialed_exact_origin_with_allow_credentials_passes() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Credentials", "true"),
            ("Set-Cookie", "session=1"),
        ],
        b"data",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        cookie_jar: Some(jar()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn same_origin_needs_no_acao_headers() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(base.clone()),
        ..options()
    };
    assert!(fetch(&base_url(&base).join("/page").unwrap(), &options).is_ok());
    let _ = server.join();
}

#[test]
fn no_origin_means_no_origin_header_and_no_cors_checks() {
    let first = ok(&[], b"data");
    let (base, server) = spawn_server(vec![first]);
    assert!(fetch(&base_url(&base).join("/api").unwrap(), &options()).is_ok());
    let requests = server.join().expect("server thread");
    assert!(!request_text(&requests[0]).contains("Origin:"));
}

#[test]
fn preflight_ok_validates_method_and_headers() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Methods", "PUT"),
            ("Access-Control-Allow-Headers", "x-token"),
        ],
        b"",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    preflight(
        &base_url(&base).join("/api").unwrap(),
        &options,
        "PUT",
        &[("X-Token", "secret")],
    )
    .expect("preflight allowed");
    let requests = server.join().expect("server thread");
    let request = request_text(&requests[0]);
    assert!(
        request.starts_with("OPTIONS /api HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request.contains(&format!("Origin: {APP_ORIGIN}")),
        "{request}"
    );
    assert!(
        request.contains("Access-Control-Request-Method: PUT"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("access-control-request-headers: x-token"),
        "{request}"
    );
    assert!(
        !request.contains("Cookie:"),
        "preflights must not send cookies"
    );
}

#[test]
fn preflight_denied_method_is_a_cors_error() {
    let first = ok(
        &[
            ("Access-Control-Allow-Origin", APP_ORIGIN),
            ("Access-Control-Allow-Methods", "GET"),
        ],
        b"",
    );
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error = preflight(
        &base_url(&base).join("/api").unwrap(),
        &options,
        "DELETE",
        &[],
    )
    .unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("DELETE")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn preflight_missing_acao_is_a_cors_error() {
    let first = ok(&[("Access-Control-Allow-Methods", "PUT")], b"");
    let (base, server) = spawn_server(vec![first]);
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    let error =
        preflight(&base_url(&base).join("/api").unwrap(), &options, "PUT", &[]).unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("Allow-Origin")),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn preflight_requires_configured_origin() {
    let error = preflight(
        &base_url("http://127.0.0.1:9").join("/api").unwrap(),
        &options(),
        "PUT",
        &[],
    )
    .unwrap_err();
    assert!(
        matches!(error, FetchError::Cors(ref reason) if reason.contains("origin")),
        "{error}"
    );
}

#[test]
fn simple_request_needs_no_preflight() {
    let (base, server) = spawn_server(Vec::new());
    let options = FetchOptions {
        origin: Some(APP_ORIGIN.to_owned()),
        ..options()
    };
    preflight(&base_url(&base).join("/api").unwrap(), &options, "GET", &[]).expect("no preflight");
    let requests = server.join().expect("server thread");
    assert!(
        requests.is_empty(),
        "simple method issued a preflight request"
    );
}
