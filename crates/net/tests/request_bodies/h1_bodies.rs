use crate::*;

#[test]
fn h1_post_uses_content_length() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(head.starts_with("POST /echo HTTP/1.1\r\n"), "{head}");
        assert!(
            head.to_ascii_lowercase().contains("content-length: 7"),
            "{head}"
        );
        let mut body = [0u8; 7];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"payload");
        ok_response(stream, b"created");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/echo")).expect("url");
    let response = fetch_with_method(&url, &options, "POST", &[], Some(b"payload")).expect("POST");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"created");
    let _ = handle.join();
}

#[test]
fn h1_post_chunked_request() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(
            head.to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "{head}"
        );
        // One chunk "abc" then the terminal chunk.
        let mut chunks = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let mut line = Vec::new();
            while !line.ends_with(b"\r\n") {
                stream.read_exact(&mut byte).expect("chunk line");
                line.push(byte[0]);
            }
            let size_text = String::from_utf8_lossy(&line[..line.len() - 2]).to_string();
            let size = usize::from_str_radix(size_text.trim(), 16).expect("chunk size");
            if size == 0 {
                // Consume the final CRLF.
                let mut crlf = [0u8; 2];
                stream.read_exact(&mut crlf).expect("trailer end");
                break;
            }
            let mut chunk = vec![0u8; size];
            stream.read_exact(&mut chunk).expect("chunk");
            chunks.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            stream.read_exact(&mut crlf).expect("chunk crlf");
        }
        assert_eq!(chunks, b"chunked-body");
        ok_response(stream, b"ok");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/chunked")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Transfer-Encoding", "chunked")],
        Some(b"chunked-body"),
    )
    .expect("chunked POST");
    assert_eq!(response.body, b"ok");
    let _ = handle.join();
}

#[test]
fn h1_100_continue_sends_body_only_after_interim() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(
            head.to_ascii_lowercase().contains("expect: 100-continue"),
            "{head}"
        );
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("interim");
        stream.flush().expect("flush");
        let mut body = [0u8; 6];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"secret");
        ok_response(stream, b"done");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/expect")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect("100-continue POST");
    assert_eq!(response.body, b"done");
    let _ = handle.join();
}

#[test]
fn h1_chunked_response_trailers_are_exposed() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                  5\r\nhello\r\n0\r\nx-checksum: abc123\r\n\r\n",
            )
            .expect("chunked response");
        stream.flush().expect("flush");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/trailers")).expect("url");
    let response = fetch_with_method(&url, &options, "GET", &[], None).expect("GET");
    assert_eq!(response.body, b"hello");
    assert_eq!(
        response.trailers,
        vec![("x-checksum".to_owned(), "abc123".to_owned())]
    );
    let _ = handle.join();
}

#[test]
fn loader_fetch_with_method_posts_and_returns_resource() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(head.starts_with("PUT /resource HTTP/1.1"), "{head}");
        let mut body = [0u8; 4];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"data");
        ok_response(stream, b"stored");
    });
    let url = Url::parse(&format!("http://{authority}/resource")).expect("url");
    let mut loader = megi_net::ResourceLoader::new();
    let resource = loader
        .fetch_with_method(&url, "PUT", &[], Some(b"data"))
        .expect("loader PUT");
    assert_eq!(resource.bytes, b"stored");
    assert_eq!(resource.url, url);
    let _ = handle.join();
}

#[test]
fn h1_103_early_hints_before_100_continue_still_sends_body() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let head = read_head(stream);
        assert!(head.contains("Expect: 100-continue\r\n"), "{head}");
        stream
            .write_all(b"HTTP/1.1 103 Early Hints\r\nLink: </app.css>; rel=preload\r\n\r\n")
            .expect("103");
        stream.flush().expect("flush");
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("100");
        stream.flush().expect("flush");
        let mut body = [0u8; 6];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"secret");
        ok_response(stream, b"done");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/hints-then-continue")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect("103 then 100-continue POST");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"done");
    let _ = handle.join();
}

#[test]
fn h1_100_continue_then_103_early_hints_returns_final_200() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("100");
        stream.flush().expect("flush");
        let mut body = [0u8; 6];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"secret");
        stream
            .write_all(b"HTTP/1.1 103 Early Hints\r\nLink: </app.js>; rel=preload\r\n\r\n")
            .expect("103");
        stream.flush().expect("flush");
        ok_response(stream, b"after-hints");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/continue-then-hints")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect("100-continue then 103 POST");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"after-hints");
    let _ = handle.join();
}

#[test]
fn h1_103_early_hints_followed_by_417_aborts_without_body() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        stream
            .write_all(b"HTTP/1.1 103 Early Hints\r\nLink: </style.css>; rel=preload\r\n\r\n")
            .expect("103");
        stream.flush().expect("flush");
        stream
            .write_all(b"HTTP/1.1 417 Expectation Failed\r\nContent-Length: 4\r\n\r\nnope")
            .expect("417");
        stream.flush().expect("flush");
        // Verify client closed/did not send the request body after 417.
        let mut extra = [0u8; 1];
        assert_eq!(stream.read(&mut extra).unwrap_or(0), 0);
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/hints-then-417")).expect("url");
    let response = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect("103 then 417 POST");
    assert_eq!(response.status, 417);
    assert_eq!(response.body, b"nope");
    let _ = handle.join();
}

#[test]
fn h1_regular_request_ignores_103_early_hints() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        stream
            .write_all(b"HTTP/1.1 103 Early Hints\r\nLink: </style.css>; rel=preload\r\n\r\n")
            .expect("103");
        stream.flush().expect("flush");
        ok_response(stream, b"final");
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/regular-hints")).expect("url");
    let response = fetch_with_method(&url, &options, "GET", &[], None).expect("GET after 103");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"final");
    let _ = handle.join();
}

#[test]
fn h1_excessive_interim_1xx_responses_are_rejected() {
    let (listener, authority) = listener();
    let handle = h1_server(listener, |stream| {
        let _ = read_head(stream);
        for _ in 0..20 {
            if stream
                .write_all(b"HTTP/1.1 103 Early Hints\r\n\r\n")
                .is_err()
            {
                break;
            }
        }
        let _ = stream.flush();
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/too-many-1xx")).expect("url");
    let err = fetch_with_method(
        &url,
        &options,
        "POST",
        &[("Expect", "100-continue")],
        Some(b"secret"),
    )
    .expect_err("too many 1xx interim heads must fail");
    assert!(
        matches!(err, megi_net::FetchError::InvalidResponse(_)),
        "expected InvalidResponse, got {err:?}"
    );
    let _ = handle.join();
}
