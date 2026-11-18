use crate::*;

#[test]
fn invalid_max_frame_size_setting_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // The client writes preface + SETTINGS + HEADERS without waiting for the server
        // SETTINGS, so read the request before answering: closing the socket first races
        // that write and surfaces EPIPE instead of the typed error below.
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_FRAME_SIZE, 1024)]);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("invalid MAX_FRAME_SIZE");
    server.join().expect("server");
    assert!(error.to_string().contains("MAX_FRAME_SIZE"), "{error}");
}

#[test]
fn invalid_initial_window_setting_is_a_typed_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // Read the request before answering (see the MAX_FRAME_SIZE test above).
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::INITIAL_WINDOW_SIZE, 0x8000_0000)]);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("invalid INITIAL_WINDOW_SIZE");
    server.join().expect("server");
    assert!(error.to_string().contains("INITIAL_WINDOW_SIZE"), "{error}");
}

#[test]
fn server_enable_push_setting_is_rejected() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        // Read the request before answering (see the MAX_FRAME_SIZE test above).
        let _ = read_request_headers(&mut tcp);
        send_settings(&mut tcp, &[(settings::ENABLE_PUSH, 1)]);
        let _ = tcp.shutdown(Shutdown::Write);
        // Keep the socket open briefly so the client observes the SETTINGS error
        // instead of racing a BrokenPipe on its preface/request writes.
        std::thread::sleep(std::time::Duration::from_millis(150));
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("server push setting");
    server.join().expect("server");
    assert!(error.to_string().contains("ENABLE_PUSH"), "{error}");
}

#[test]
fn rst_one_stream_does_not_affect_the_other() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[(settings::MAX_CONCURRENT_STREAMS, 4)]);
        let (first, _) = read_request_headers(&mut tcp);
        let (second, _) = read_request_headers(&mut tcp);
        assert_eq!((first, second), (1, 3));
        let mut frames = vec![response_header_frame(200, &[], 1, false)];
        frames.push(Frame::rst_stream(1, error_code::CANCEL));
        frames.extend(response_with_body(200, &[], 3, b"kept"));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let requests = [
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/reset", &[]),
        H2Request::get("http", &format!("127.0.0.1:{port}"), "/kept", &[]),
    ];
    let results = client::get_many(&mut stream, &requests, &Limits::default()).expect("batch");
    server.join().expect("server");
    assert!(
        results[0].as_ref().is_err(),
        "RST stream must fail its own request"
    );
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("reset"),
        "{:?}",
        results[0]
    );
    assert_eq!(results[1].as_ref().expect("second stream").body, b"kept");
}

#[test]
fn goaway_mid_batch_is_a_typed_connection_error() {
    let (port, server) = spawn_server(|mut tcp| {
        read_preface(&mut tcp);
        send_settings(&mut tcp, &[]);
        let (stream, _) = read_request_headers(&mut tcp);
        let mut frames = vec![response_header_frame(200, &[], stream, false)];
        frames.push(Frame::goaway(stream, error_code::NO_ERROR, b"bye"));
        send_frames(&mut tcp, &frames);
        let _ = tcp.shutdown(Shutdown::Write);
    });
    let mut stream = connect(port);
    let request = H2Request::get("http", &format!("127.0.0.1:{port}"), "/", &[]);
    let error = client::get_many(&mut stream, &[request], &Limits::default())
        .expect_err("GOAWAY mid-stream");
    server.join().expect("server");
    assert!(matches!(error, FetchError::Http2(_)), "{error}");
    assert!(error.to_string().contains("GOAWAY"), "{error}");
}
