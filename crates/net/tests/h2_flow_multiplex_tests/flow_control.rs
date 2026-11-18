use crate::*;

#[test]
fn multiplexes_two_streams_with_interleaved_frames() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(
            &mut tcp,
            &[
                (settings::MAX_CONCURRENT_STREAMS, 4),
                (settings::INITIAL_WINDOW_SIZE, 65_535),
            ],
        );
        let (stream_a, headers_a) = read_request_headers(&mut tcp);
        let (stream_b, headers_b) = read_request_headers(&mut tcp);
        assert_eq!(
            (stream_a, stream_b),
            (1, 3),
            "client must use odd stream ids"
        );
        assert!(
            headers_a
                .iter()
                .any(|(name, value)| name == ":path" && value == "/a")
        );
        assert!(
            headers_b
                .iter()
                .any(|(name, value)| name == ":path" && value == "/b")
        );

        let frames = vec![
            response_header_frame(200, &[("x-stream", "a")], 1, false),
            Frame::data(1, b"AAA", false),
            response_header_frame(200, &[], 3, false),
            Frame::data(3, b"BBB", true),
            Frame::data(1, b"CCC", true),
        ];
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/a", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/b", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results.len(), 2);
    let first = results[0].as_ref().expect("stream 1");
    assert_eq!(first.status, 200);
    assert_eq!(first.body, b"AAACCC");
    assert_eq!(first.headers, vec![("x-stream".to_owned(), "a".to_owned())]);
    let second = results[1].as_ref().expect("stream 3");
    assert_eq!(second.status, 200);
    assert_eq!(second.body, b"BBB");
}

#[test]
fn large_body_waits_for_window_updates_and_completes() {
    let body = vec![0x5Au8; 200_000];
    let expected = body.clone();
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _headers) = read_request_headers(&mut tcp);
        send_frames(
            &mut tcp,
            &[response_header_frame(
                200,
                &[("content-length", &body.len().to_string())],
                stream,
                false,
            )],
        );
        let mut connection_window = 65_535i64;
        let mut stream_window = 65_535i64;
        let mut sent = 0usize;
        while sent < body.len() {
            if connection_window == 0 || stream_window == 0 {
                // Consume WINDOW_UPDATEs from the client before sending more DATA.
                loop {
                    let frame = read_frame(&mut tcp);
                    if frame.header.kind != kind::WINDOW_UPDATE {
                        continue;
                    }
                    let increment = i64::from(frame.window_increment().expect("increment"));
                    assert!(
                        increment > 0,
                        "WINDOW_UPDATE must replenish with a positive delta"
                    );
                    if frame.header.stream_id == 0 {
                        connection_window += increment;
                    } else {
                        stream_window += increment;
                    }
                    if connection_window > 0 && stream_window > 0 {
                        break;
                    }
                }
                continue;
            }
            let remaining = body.len() - sent;
            let chunk = 16_384i64
                .min(connection_window)
                .min(stream_window)
                .min(remaining as i64) as usize;
            let end = sent + chunk == body.len();
            send_frames(
                &mut tcp,
                &[Frame::data(stream, &body[sent..sent + chunk], end)],
            );
            sent += chunk;
            connection_window -= chunk as i64;
            stream_window -= chunk as i64;
        }
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/large", &[]);
    let limits = Limits {
        max_body: 1_000_000,
        ..Limits::default()
    };
    let results = client::get_many(&mut stream, &[request], &limits).expect("large body");
    server.join().expect("server");
    let response = results[0].as_ref().expect("response");
    assert_eq!(response.body.len(), 200_000);
    assert_eq!(response.body, expected);
}

#[test]
fn flow_control_violation_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut bytes = Vec::new();
        response_header_frame(200, &[], stream, false).write_to(&mut bytes);
        Frame::data(stream, &vec![0u8; 2048], false).write_to(&mut bytes);
        let _ = tcp.write_all(&bytes);
        let _ = tcp.flush();
        let _ = tcp.shutdown(Shutdown::Both);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let limits = Limits {
        initial_window_size: 1024,
        ..Limits::default()
    };
    let error = client::get_many(&mut stream, &[request], &limits).expect_err("window violation");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    assert!(
        error
            .to_string()
            .to_ascii_lowercase()
            .contains("flow_control"),
        "{error}"
    );
    let _ = server.join();
}

#[test]
fn zero_window_update_increment_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut frames = vec![response_header_frame(200, &[], stream, false)];
        frames.push(Frame::window_update(0, 0));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("zero WINDOW_UPDATE");
    server.join().expect("server");
    assert!(error.to_string().contains("zero"), "{error}");
}
