use crate::*;

#[test]
fn h2_post_small_body_and_end_stream() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let server_captured = std::sync::Arc::clone(&captured);
    let limits = Limits::default();
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/submit",
            &[],
            Some(b"hello h2".to_vec()),
        ),
        limits,
        move |stream| {
            *server_captured.lock().expect("lock") = collect_request_body(stream).0;
            response_headers(stream, 1, "200", false);
            send_frame(stream, &Frame::data(1, b"ok", true));
        },
    )
    .expect("session");
    assert_eq!(&*captured.lock().expect("lock"), b"hello h2");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}

#[test]
fn h2_large_body_fragmented_and_window_updated() {
    let body = vec![7u8; 200_000];
    let limits = Limits::default();
    let expected = body.clone();
    let results = h2_exchange(
        H2Request::new("POST", "http", "example", "/upload", &[], Some(body)),
        limits,
        move |stream| {
            send_frame(stream, &Frame::settings(&[]));
            let mut received = Vec::new();
            let mut largest = 0usize;
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::DATA => {
                        largest = largest.max(frame.payload.len());
                        received.extend_from_slice(&frame.payload);
                        let increment = frame.payload.len() as u32;
                        send_frame(stream, &Frame::window_update(0, increment));
                        send_frame(stream, &Frame::window_update(1, increment));
                        if frame.header.flags & flags::END_STREAM != 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(received, expected);
            assert!(
                largest <= 16_384,
                "DATA chunk {largest} exceeds MAX_FRAME_SIZE"
            );
            response_headers(stream, 1, "200", true);
        },
    )
    .expect("session");
    assert_eq!(results[0].as_ref().expect("response").status, 200);
}

#[test]
fn h2_response_trailers_are_exposed() {
    let results = h2_exchange(
        H2Request::get("http", "example", "/trailers", &[]),
        Limits::default(),
        move |stream| {
            // Drain the request before answering: closing a socket with unread bytes makes
            // the kernel send RST, which can discard the trailers before the client reads
            // them (BrokenPipe / "connection closed before all responses completed").
            let _ = collect_request_body(stream);
            response_headers(stream, 1, "200", false);
            send_frame(stream, &Frame::data(1, b"payload", false));
            trailer_headers(stream, 1, &[("x-checksum", "abc123"), ("x-done", "yes")]);
        },
    )
    .expect("session");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.body, b"payload");
    assert_eq!(
        response.trailers,
        vec![
            ("x-checksum".to_owned(), "abc123".to_owned()),
            ("x-done".to_owned(), "yes".to_owned())
        ]
    );
}

#[test]
fn h2_100_continue_delays_the_body() {
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/continue",
            &[("Expect".to_owned(), "100-continue".to_owned())],
            Some(b"after-continue".to_vec()),
        ),
        Limits::default(),
        move |stream| {
            // HEADERS must arrive first (skip the client SETTINGS), without DATA.
            let mut frame;
            loop {
                frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::HEADERS => break,
                    other => panic!("unexpected frame {other}"),
                }
            }
            assert_eq!(frame.header.flags & flags::END_STREAM, 0);
            // Interim 100 response unlocks the body.
            let block = Encoder::new().encode(&[(":status".to_owned(), "100".to_owned())]);
            send_frame(stream, &Frame::headers(1, &block, false));
            let (body, end_seen, _) = collect_request_body(stream);
            assert_eq!(body, b"after-continue");
            assert!(end_seen);
            response_headers(stream, 1, "201", true);
        },
    )
    .expect("session");
    assert_eq!(results[0].as_ref().expect("response").status, 201);
}

#[test]
fn h2_rst_stream_while_sending_body_is_a_typed_stream_error() {
    let results = h2_exchange(
        H2Request::new(
            "POST",
            "http",
            "example",
            "/reset",
            &[],
            Some(vec![1u8; 50_000]),
        ),
        Limits::default(),
        move |stream| {
            loop {
                let frame = read_frame(stream);
                match frame.header.kind {
                    kind::SETTINGS => {
                        if frame.header.flags & flags::ACK == 0 {
                            send_settings_ack(stream);
                        }
                    }
                    kind::HEADERS => break,
                    _ => {}
                }
            }
            send_frame(stream, &Frame::rst_stream(1, frame::error_code::CANCEL));
            // Keep the connection open briefly so the client can read the RST instead
            // of racing a BrokenPipe on its DATA writes.
            std::thread::sleep(std::time::Duration::from_millis(500));
        },
    )
    .expect("session");
    let error = results[0].as_ref().expect_err("RST must fail the stream");
    assert!(matches!(error, FetchError::Http2(_)), "{error:?}");
}

#[test]
fn h2_request_body_cap_is_enforced_without_panic() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        read_preface(&mut stream);
        let _ = stream.read(&mut [0u8; 1]);
    });
    let mut stream = TcpStream::connect(authority).expect("connect");
    let limits = Limits {
        max_request_body: 8,
        ..Limits::default()
    };
    let request = H2Request::new("POST", "http", "example", "/", &[], Some(vec![0u8; 9]));
    let results = request_many(&mut stream, &[request], &limits).expect("session");
    let error = results[0].as_ref().expect_err("body cap");
    assert!(
        matches!(error, FetchError::RequestBodyTooLarge),
        "{error:?}"
    );
    let _ = handle.join();
}

#[test]
fn h2_truncated_body_is_a_typed_error() {
    let results = h2_exchange(
        H2Request::get("http", "example", "/truncated", &[]),
        Limits::default(),
        move |stream| {
            response_headers(stream, 1, "200", false);
            // Declare a DATA frame but write only part of it, then close.
            stream
                .write_all(&[0, 0, 5, 0x00, 0x01, 0, 0, 0, 1])
                .expect("frame head");
            stream.write_all(b"ab").expect("partial data");
            stream.flush().expect("flush");
        },
    );
    assert!(results.is_err(), "truncated frame must fail the connection");
}
