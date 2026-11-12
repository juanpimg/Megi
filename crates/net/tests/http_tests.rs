//! Offline HTTP/1.1 client tests.
//!
//! Every test binds its own [`std::net::TcpListener`] on `127.0.0.1:0`, serves hand-written
//! responses from a worker thread and then joins the thread. No external network, no
//! DNS lookups beyond `127.0.0.1`, and no long sleeps: the only timeouts used are short
//! socket timeouts that act as a safety net so a regression fails instead of hanging.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::thread::JoinHandle;
use std::time::Duration;

use net::{FetchError, FetchOptions, Url, fetch};

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

#[test]
fn http_200_with_content_length() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 5\r\n\r\nhello"
            .to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/index.html").expect("join");
    let response = fetch(&url, &options()).expect("fetch succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.reason, "OK");
    assert_eq!(response.body, b"hello");
    assert_eq!(response.content_type(), Some("text/html; charset=utf-8"));
    assert!(response.is_success());
    assert_eq!(response.final_url.path(), "/index.html");

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 1);
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(
        request.starts_with("GET /index.html HTTP/1.1\r\n"),
        "unexpected request line: {request:?}"
    );
    assert!(
        request.contains(&format!("Host: {}", base.trim_start_matches("http://"))),
        "missing Host header: {request:?}"
    );
    assert!(request.contains("User-Agent: Megi/0.1"), "{request:?}");
    assert!(request.contains("Accept: */*"), "{request:?}");
    assert!(request.contains("Connection: close"), "{request:?}");
}

#[test]
fn http_request_target_includes_query_string() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base)
        .join("/search?a=b&c=d%20e")
        .expect("join with query");
    fetch(&url, &options()).expect("fetch succeeds");
    let requests = server.join().expect("server thread");
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(
        request.starts_with("GET /search?a=b&c=d%20e HTTP/1.1\r\n"),
        "unexpected request line: {request:?}"
    );
}

#[test]
fn http_404_returns_status_and_body() {
    let response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/missing").expect("join");
    let response = fetch(&url, &options()).expect("fetch succeeds");
    assert_eq!(response.status, 404);
    assert_eq!(response.body, b"not found");
    assert!(!response.is_success());
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_chunked_body_is_decoded() {
    let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let response = fetch(&url, &options()).expect("fetch succeeds");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello world");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_body_without_length_reads_until_eof() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nuntil-eof".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let response = fetch(&url, &options()).expect("fetch succeeds");
    assert_eq!(response.body, b"until-eof");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_redirect_follows_relative_location() {
    let responses = vec![
        b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
    ];
    let (base, server) = spawn_server(responses);
    let url = base_url(&base).join("/start").expect("join");
    let response = fetch(&url, &options()).expect("fetch succeeds");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
    assert_eq!(response.final_url.path(), "/final");

    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        String::from_utf8_lossy(&requests[1]).starts_with("GET /final HTTP/1.1\r\n"),
        "redirect must target /final"
    );
}

#[test]
fn http_too_many_redirects_is_an_error() {
    let redirect =
        b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /loop\r\nContent-Length: 0\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![redirect.clone(), redirect.clone(), redirect]);
    let url = base_url(&base).join("/loop").expect("join");
    let options = FetchOptions {
        max_redirects: 2,
        ..options()
    };
    let error = fetch(&url, &options).expect_err("redirect loop must fail");
    assert!(matches!(error, FetchError::TooManyRedirects), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 3);
}

#[test]
fn http_redirect_without_location_is_an_error() {
    let response = b"HTTP/1.1 301 Moved Permanently\r\nContent-Length: 0\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let error = fetch(&url, &options()).expect_err("redirect without location must fail");
    assert!(
        matches!(error, FetchError::RedirectWithoutLocation),
        "{error}"
    );
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_response_too_large_by_content_length() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let options = FetchOptions {
        max_response_bytes: 16,
        ..options()
    };
    let error = fetch(&url, &options).expect_err("large body must be rejected");
    assert!(matches!(error, FetchError::ResponseTooLarge), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_truncated_body_is_an_error() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let error = fetch(&url, &options()).expect_err("truncated body must fail");
    assert!(matches!(error, FetchError::BodyIncomplete), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_headers_too_large_are_rejected() {
    let mut response = Vec::from(&b"HTTP/1.1 200 OK\r\nX-Long: "[..]);
    response.extend(std::iter::repeat_n(b'a', 512));
    response.extend_from_slice(b"\r\nContent-Length: 0\r\n\r\n");
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let options = FetchOptions {
        max_header_bytes: 64,
        ..options()
    };
    let error = fetch(&url, &options).expect_err("large headers must be rejected");
    assert!(matches!(error, FetchError::HeadersTooLarge), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_invalid_status_line_is_rejected() {
    let response = b"NOT-HTTP 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let error = fetch(&url, &options()).expect_err("invalid status line must fail");
    assert!(matches!(error, FetchError::InvalidResponse(_)), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn http_header_line_folding_is_rejected() {
    let response = b"HTTP/1.1 200 OK\r\nX-A: 1\r\n folded\r\nContent-Length: 0\r\n\r\n".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base);
    let error = fetch(&url, &options()).expect_err("folded headers must fail");
    assert!(matches!(error, FetchError::InvalidResponse(_)), "{error}");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[cfg(not(feature = "tls"))]
#[test]
fn https_urls_report_tls_unsupported_without_connecting() {
    let url = Url::parse("https://example.com/").expect("https URL parses");
    let error = fetch(&url, &options()).expect_err("https is not supported yet");
    assert!(matches!(error, FetchError::TlsUnsupported), "{error}");
}

#[test]
fn file_and_unsupported_schemes_are_not_fetched_over_http() {
    let url = Url::parse("file:///tmp/x").expect("file URL parses");
    assert!(matches!(
        fetch(&url, &options()),
        Err(FetchError::UnsupportedScheme(scheme)) if scheme == "file"
    ));
}

#[test]
fn chunked_chunk_size_overflow_is_rejected() {
    // `FFFFFFFFFFFFFFFF` is a valid hex chunk size that overflows usize arithmetic when
    // added to the bytes already buffered (RFC 9112 §7.1).
    let response =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nA\r\nFFFFFFFFFFFFFFFF\r\n"
            .to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/overflow").expect("join");

    let error = fetch(&url, &options()).expect_err("overflowing chunk size");
    assert!(
        matches!(error, FetchError::ResponseTooLarge),
        "expected ResponseTooLarge, got {error}"
    );
    let _ = server.join();
}

#[test]
fn oversized_trailer_section_is_rejected() {
    let mut response = String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n");
    for index in 0..40 {
        response.push_str(&format!("x-t{index}: {}\r\n", "a".repeat(40)));
    }
    response.push_str("\r\n");
    let (base, server) = spawn_server(vec![response.into_bytes()]);
    let url = base_url(&base).join("/trailers").expect("join");
    let small_headers = FetchOptions {
        max_header_bytes: 256,
        ..options()
    };

    let error = fetch(&url, &small_headers).expect_err("oversized trailer section");
    assert!(
        matches!(error, FetchError::HeadersTooLarge),
        "expected HeadersTooLarge, got {error}"
    );
    let _ = server.join();
}

#[test]
fn conflicting_content_length_headers_are_rejected() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 5\r\n\r\nhello".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/smuggled").expect("join");

    let error = fetch(&url, &options()).expect_err("conflicting Content-Length");
    assert!(
        matches!(error, FetchError::InvalidResponse(_)),
        "expected InvalidResponse, got {error}"
    );
    let _ = server.join();
}

#[test]
fn duplicate_identical_content_length_headers_are_accepted() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/duplicate").expect("join");

    let response = fetch(&url, &options()).expect("identical duplicates are valid");
    assert_eq!(response.body, b"hello");
    let _ = server.join();
}

#[test]
fn streaming_trailer_section_is_capped() {
    let mut response = String::from("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n");
    for index in 0..40 {
        response.push_str(&format!("x-t{index}: {}\r\n", "a".repeat(40)));
    }
    response.push_str("\r\n");
    let (base, server) = spawn_server(vec![response.into_bytes()]);
    let url = base_url(&base).join("/stream-trailers").expect("join");
    let small_headers = FetchOptions {
        max_header_bytes: 256,
        ..options()
    };

    let mut chunks = 0usize;
    let error = net::fetch_streaming(&url, &small_headers, &mut |_chunk| {
        chunks += 1;
        Ok(())
    })
    .expect_err("streaming trailer cap");
    assert!(
        matches!(error, FetchError::HeadersTooLarge),
        "expected HeadersTooLarge, got {error}"
    );
    assert_eq!(chunks, 0, "the trailer error must abort before data");
    let _ = server.join();
}

#[test]
fn streaming_conflicting_content_length_is_rejected() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 5\r\n\r\nhello".to_vec();
    let (base, server) = spawn_server(vec![response]);
    let url = base_url(&base).join("/stream-smuggled").expect("join");

    let error = net::fetch_streaming(&url, &options(), &mut |_chunk| Ok(()))
        .expect_err("streaming conflicting Content-Length");
    assert!(
        matches!(error, FetchError::InvalidResponse(_)),
        "expected InvalidResponse, got {error}"
    );
    let _ = server.join();
}

#[test]
fn request_headers_are_validated_before_connecting() {
    let url = Url::parse("http://127.0.0.1:9/").expect("url");
    let options = options();
    let cases = [
        ("Host", "evil.example"),
        ("X-Test", "ok\r\nHost: evil.example"),
        ("X-Test", "ok\nX-Injected: yes"),
        ("X Test", "ok"),
        ("X-Test", "bad\u{0}value"),
        ("X-Test", "bad\u{7f}value"),
    ];
    for (name, value) in cases {
        let error = net::http::fetch_with_method(&url, &options, "GET", &[(name, value)], None)
            .expect_err("header must be rejected before the connection");
        assert!(
            matches!(error, FetchError::InvalidRequestHeader(_)),
            "{name:?}: {error}"
        );
    }
}
