//! Unit tests kept alongside the HTTP client (moved out of `http.rs` for file-size limits).

#[cfg(test)]
#[cfg(feature = "tls")]
mod tls_cache_tests {
    use crate::http::tls::default_tls_config;
    use std::sync::Arc;

    #[test]
    fn successful_tls_config_is_cached() {
        let a = default_tls_config().expect("first build");
        let b = default_tls_config().expect("cached build");
        assert!(
            Arc::ptr_eq(&a, &b),
            "successful configurations must be shared across fetches"
        );
    }
}

#[cfg(test)]
mod header_encoding_tests {
    use crate::http::response::parse_head_verbose;

    #[test]
    fn latin1_header_bytes_are_preserved() {
        let head = b"HTTP/1.1 200 OK\r\nX-City: Z\xFCrich\r\n\r\n";
        let (_major, status, reason, headers) = parse_head_verbose(head).expect("parse");
        assert_eq!(status, 200);
        assert_eq!(reason, "OK");
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("x-city"))
                .map(|(_, value)| value.as_str()),
            Some("Z\u{00FC}rich")
        );
    }
}

#[cfg(test)]
mod parse_head_validity_tests {
    use crate::http::response::parse_head_is_valid;

    #[test]
    fn head_validity_matches_parser() {
        assert!(parse_head_is_valid(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(!parse_head_is_valid(b"garbage\r\n\r\n"));
        assert!(!parse_head_is_valid(b""));
    }
}

#[cfg(test)]
mod write_timeout_tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use crate::body::RequestBody;
    use crate::error::FetchError;
    use crate::http::transport::connect_tcp;
    use crate::http::{FetchOptions, fetch_with_request_body};
    use crate::url::Url;

    #[test]
    fn connect_tcp_honors_write_timeout_independently() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let _accepted = listener.accept();
            let _accepted2 = listener.accept();
        });

        let custom = FetchOptions {
            connect_timeout: Duration::from_secs(7),
            read_timeout: Duration::from_secs(9),
            write_timeout: Some(Duration::from_millis(150)),
            ..FetchOptions::default()
        };
        let stream = connect_tcp("127.0.0.1", port, &custom).expect("connect");
        assert_eq!(stream.read_timeout().unwrap(), Some(Duration::from_secs(9)));
        // Socket timeouts are rounded up to the OS clock granularity, so an
        // exact comparison is flaky (observed 152 ms for a 150 ms request).
        let write_timeout = stream
            .write_timeout()
            .unwrap()
            .expect("explicit write timeout is set");
        assert!(
            (Duration::from_millis(140)..=Duration::from_millis(250)).contains(&write_timeout),
            "explicit write_timeout must be applied independently of read/connect timeouts (#1635): {write_timeout:?}"
        );
        assert!(
            write_timeout < custom.read_timeout,
            "write timeout must not inherit the longer read timeout (#1635)"
        );

        let fallback = FetchOptions {
            connect_timeout: Duration::from_secs(7),
            read_timeout: Duration::from_secs(4),
            write_timeout: None,
            ..FetchOptions::default()
        };
        let stream2 = connect_tcp("127.0.0.1", port, &fallback).expect("connect fallback");
        assert_eq!(
            stream2.write_timeout().unwrap(),
            Some(Duration::from_secs(4)),
            "write_timeout: None must fall back to read_timeout for HTTP transport (#1635)"
        );
        let _ = server.join();
    }

    #[test]
    fn http_streaming_upload_aborts_on_write_timeout_not_read_timeout() {
        // #1635: a slow peer that never drains its receive buffer must trigger write_timeout
        // quickly even when connect_timeout and read_timeout are much longer.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Read only the HTTP request headers, then stop reading so kernel TCP buffers fill.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && head.len() < 4096 {
                if stream.read(&mut byte).unwrap_or(0) != 1 {
                    break;
                }
                head.push(byte[0]);
            }
            let _ = done_rx.recv_timeout(Duration::from_secs(6));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        });

        let url = Url::parse(&format!("http://127.0.0.1:{port}/upload")).expect("url");
        let options = FetchOptions {
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            write_timeout: Some(Duration::from_millis(50)),
            max_request_body: 64 * 1024 * 1024,
            ..FetchOptions::default()
        };
        let mut sent = 0usize;
        let body = RequestBody::stream(move |max: usize| {
            if sent >= 64 * 1024 * 1024 {
                return Ok(None);
            }
            let chunk_len = max.min(64 * 1024);
            sent += chunk_len;
            Ok(Some(vec![b'a'; chunk_len]))
        });
        let start = Instant::now();
        let err = fetch_with_request_body(&url, &options, "POST", &[], Some(body))
            .expect_err("stalled upload must time out on write_timeout");
        let elapsed = start.elapsed();
        let _ = done_tx.send(());
        let _ = server.join();
        assert!(
            matches!(err, FetchError::Timeout),
            "expected Timeout on write stall, got {err:?}"
        );
        assert!(
            elapsed < Duration::from_millis(750),
            "write stall took {elapsed:?}, expected ~50ms write_timeout instead of 5s read_timeout"
        );
    }
}
