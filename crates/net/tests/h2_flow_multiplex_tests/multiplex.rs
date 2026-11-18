use crate::*;

#[test]
fn large_request_headers_are_fragmented_into_continuations() {
    // Pseudo-random printable ASCII: deterministic and poorly compressible under HPACK
    // Huffman, so the encoded block exceeds the 16 KiB default MAX_FRAME_SIZE.
    let big_value: String = (0..40_000)
        .map(|index| {
            let byte = 33 + ((index * 131 + 17) % 90) as u8;
            char::from(byte)
        })
        .collect();
    let expected_value = big_value.clone();
    let (port, server) = spawn_server(move |mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        // Read frames until END_HEADERS, counting HEADERS + CONTINUATION fragments.
        let mut assembler = HeaderAssembler::default();
        let mut fragments = 0usize;
        let request_headers: Vec<(String, String)> = loop {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::SETTINGS => {
                    if frame.header.flags & flags::ACK == 0 {
                        send_frames(&mut tcp, &[Frame::settings_ack()]);
                    }
                }
                kind::HEADERS | kind::CONTINUATION => {
                    fragments += 1;
                    if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                        break Decoder::default().decode(&block).expect("HPACK");
                    }
                }
                _ => {}
            }
        };
        assert!(
            fragments >= 2,
            "header block larger than MAX_FRAME_SIZE must be split"
        );
        assert!(
            request_headers
                .iter()
                .any(|(name, value)| name == "x-large" && value == &expected_value)
        );
        let stream_id = 1;
        send_frames(&mut tcp, &response_with_body(200, &[], stream_id, b"ok"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get(
        "http",
        &format!("127.0.0.1:{port}"),
        "/big-header",
        &[("x-large".to_owned(), big_value)],
    );
    let results = client::get_many(&mut stream, &[request], &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").body, b"ok");
}

#[test]
fn peer_max_concurrent_streams_is_respected() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_CONCURRENT_STREAMS, 1)]);
        let (first, _) = read_request_headers(&mut tcp);
        assert_eq!(first, 1);

        // Before answering stream 1, no second HEADERS may already be in flight: the peer cap is 1
        // and stream 1 is still open. Drain immediately-available frames non-blocking so a violating
        // eager send is caught deterministically, without a negative timing window (#1350).
        tcp.set_nonblocking(true).expect("nonblocking");
        loop {
            match FrameReader::new(&mut tcp, frame::DEFAULT_MAX_FRAME_SIZE).read_frame() {
                Ok(Some(frame)) => assert_ne!(
                    frame.header.kind,
                    kind::HEADERS,
                    "client opened a second stream despite MAX_CONCURRENT_STREAMS=1"
                ),
                Ok(None) => break,
                Err(FetchError::Timeout) => break,
                Err(error) => panic!("unexpected error while draining pending frames: {error}"),
            }
        }
        tcp.set_nonblocking(false).expect("blocking");
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("restore timeout");

        send_frames(&mut tcp, &response_with_body(200, &[], 1, b"one"));
        let (second, _) = read_request_headers(&mut tcp);
        assert_eq!(
            second, 3,
            "second request must reuse the next odd stream id"
        );
        send_frames(&mut tcp, &response_with_body(200, &[], 3, b"two"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/one", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/two", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("first").body, b"one");
    assert_eq!(results[1].as_ref().expect("second").body, b"two");
}

#[test]
fn client_acknowledges_peer_settings() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        // Read until the first request HEADERS, recording the SETTINGS ACK sent by the client.
        let mut assembler = HeaderAssembler::default();
        let mut decoder = Decoder::default();
        let mut saw_ack = false;
        let mut request: Option<(u32, Vec<(String, String)>)> = None;
        while !saw_ack || request.is_none() {
            let frame = read_frame(&mut tcp);
            match frame.header.kind {
                kind::SETTINGS if frame.header.flags & flags::ACK != 0 => saw_ack = true,
                kind::SETTINGS => send_frames(&mut tcp, &[Frame::settings_ack()]),
                kind::HEADERS | kind::CONTINUATION => {
                    let stream = frame.header.stream_id;
                    if let Some((block, _)) = assembler.feed(&frame).expect("assemble") {
                        request = Some((stream, decoder.decode(&block).expect("HPACK")));
                    }
                }
                _ => {}
            }
        }
        let (stream_id, _headers) = request.expect("request headers");
        assert!(saw_ack, "client must acknowledge the peer SETTINGS frame");
        send_frames(&mut tcp, &response_with_body(200, &[], stream_id, b"ack"));
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let results = client::get_many(&mut stream, &[request], &Limits::default()).expect("batch");
    server.join().expect("server");
    assert_eq!(results[0].as_ref().expect("response").body, b"ack");
}
