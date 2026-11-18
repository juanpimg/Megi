use crate::*;

#[test]
fn h1_streaming_body_uses_chunked_and_backpressure_counter() {
    let (listener, authority) = listener();
    let calls = Arc::new(AtomicUsize::new(0));
    let server_calls = Arc::clone(&calls);
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(head.starts_with("POST /upload HTTP/1.1"), "{head}");
        assert!(
            head.to_ascii_lowercase()
                .contains("transfer-encoding: chunked"),
            "{head}"
        );
        assert!(
            !head.to_ascii_lowercase().contains("content-length"),
            "{head}"
        );
        let (body, chunks, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"alpha-beta-gamma");
        assert_eq!(chunks.len(), 3, "three producer chunks must stay separate");
        write_response(&mut stream, b"ok");
        // The producer is polled once more to observe EOF.
        assert_eq!(server_calls.load(Ordering::Relaxed), 4);
    });
    let chunks = vec![b"alpha".to_vec(), b"-beta".to_vec(), b"-gamma".to_vec()];
    let index = Arc::new(AtomicUsize::new(0));
    let producer_chunks = chunks.clone();
    let producer_index = Arc::clone(&index);
    let producer = RequestBody::stream(move |max: usize| {
        calls.fetch_add(1, Ordering::Relaxed);
        let index = producer_index.fetch_add(1, Ordering::Relaxed);
        if index >= producer_chunks.len() {
            return Ok(None);
        }
        let chunk = producer_chunks[index].clone();
        assert!(chunk.len() <= max);
        Ok(Some(chunk))
    });
    let options = FetchOptions::default();
    let url = Url::parse(&format!("http://{authority}/upload")).expect("url");
    let response = fetch_with_request_body(&url, &options, "POST", &[], Some(producer))
        .expect("streaming POST");
    assert_eq!(response.body, b"ok");
    handle.join().expect("server");
}

#[test]
fn h1_empty_stream_sends_terminal_chunk() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let (body, chunks, _) = read_chunked_body(&mut stream);
        assert!(body.is_empty());
        assert!(chunks.is_empty());
        write_response(&mut stream, b"empty");
    });
    let body = RequestBody::stream(|_max: usize| Ok(None));
    let url = Url::parse(&format!("http://{authority}/empty")).expect("url");
    let response = fetch_with_request_body(&url, &FetchOptions::default(), "POST", &[], Some(body))
        .expect("empty stream");
    assert_eq!(response.body, b"empty");
    handle.join().expect("server");
}

#[test]
fn h1_streaming_error_aborts_with_typed_error() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let mut chunk = [0u8; 3];
        let _ = stream.read(&mut chunk);
        std::thread::sleep(Duration::from_millis(50));
    });
    let calls = AtomicUsize::new(0);
    let body = RequestBody::stream(move |_max: usize| {
        if calls.fetch_add(1, Ordering::Relaxed) == 0 {
            Ok(Some(b"abc".to_vec()))
        } else {
            Err(FetchError::RequestBody("producer boom".to_owned()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/fail")).expect("url");
    let error = fetch_with_request_body(&url, &FetchOptions::default(), "POST", &[], Some(body))
        .expect_err("producer error");
    assert!(matches!(error, FetchError::RequestBody(_)), "{error:?}");
    let _ = handle.join();
}

#[test]
fn h1_streaming_cap_is_enforced() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let mut byte = [0u8; 1];
        let _ = stream.read(&mut byte);
        std::thread::sleep(Duration::from_millis(50));
    });
    let calls = AtomicUsize::new(0);
    let body = RequestBody::stream(move |_max: usize| {
        let call = calls.fetch_add(1, Ordering::Relaxed);
        Ok(Some(vec![b'x'; 3 + call]))
    });
    let options = FetchOptions {
        max_request_body: 5,
        ..FetchOptions::default()
    };
    let url = Url::parse(&format!("http://{authority}/cap")).expect("url");
    let error = fetch_with_request_body(&url, &options, "POST", &[], Some(body)).expect_err("cap");
    assert!(
        matches!(error, FetchError::RequestBodyTooLarge),
        "{error:?}"
    );
    let _ = handle.join();
}

#[test]
fn h1_streaming_100_continue_sends_body_after_interim() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(
            head.to_ascii_lowercase().contains("expect: 100-continue"),
            "{head}"
        );
        stream
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
            .expect("interim");
        stream.flush().expect("flush");
        let (body, _, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"continue-body");
        write_response(&mut stream, b"done");
    });
    let mut sent = false;
    let body = RequestBody::stream(move |_max: usize| {
        if sent {
            Ok(None)
        } else {
            sent = true;
            Ok(Some(b"continue-body".to_vec()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/continue")).expect("url");
    let response = fetch_with_request_body(
        &url,
        &FetchOptions::default(),
        "POST",
        &[("Expect", "100-continue")],
        Some(body),
    )
    .expect("100-continue streaming");
    assert_eq!(response.body, b"done");
    handle.join().expect("server");
}

#[test]
fn h1_byte_body_still_uses_content_length() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let head = read_head(&mut stream);
        assert!(
            head.to_ascii_lowercase().contains("content-length: 4"),
            "{head}"
        );
        let mut body = [0u8; 4];
        stream.read_exact(&mut body).expect("body");
        assert_eq!(&body, b"data");
        write_response(&mut stream, b"ok");
    });
    let url = Url::parse(&format!("http://{authority}/bytes")).expect("url");
    let response = fetch_with_method(&url, &FetchOptions::default(), "POST", &[], Some(b"data"))
        .expect("bytes POST");
    assert_eq!(response.body, b"ok");
    handle.join().expect("server");
}

#[test]
fn loader_streaming_upload_returns_resource() {
    let (listener, authority) = listener();
    let handle = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let _head = read_head(&mut stream);
        let (body, _, _) = read_chunked_body(&mut stream);
        assert_eq!(body, b"loader-stream");
        write_response(&mut stream, b"stored");
    });
    let mut sent = false;
    let body = RequestBody::stream(move |_max: usize| {
        if sent {
            Ok(None)
        } else {
            sent = true;
            Ok(Some(b"loader-stream".to_vec()))
        }
    });
    let url = Url::parse(&format!("http://{authority}/loader")).expect("url");
    let mut loader = megi_net::ResourceLoader::new();
    let resource = loader
        .fetch_with_request_body(&url, "PUT", &[], Some(body))
        .expect("loader streaming");
    assert_eq!(resource.bytes, b"stored");
    handle.join().expect("server");
}
