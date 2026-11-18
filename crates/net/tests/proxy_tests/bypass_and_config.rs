use crate::*;

#[test]
fn no_proxy_bypasses_the_configured_proxy() {
    let (origin_port, origin) = spawn_http_origin(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 6\r\n\r\ndirect".to_vec(),
    );
    // The proxy URL points at an unreachable port: using it would return Connect.
    let options = FetchOptions {
        connect_timeout: TIMEOUT,
        read_timeout: TIMEOUT,
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(Url::parse("http://127.0.0.1:1/").expect("proxy URL")),
            https: None,
            no_proxy: vec!["127.0.0.1".to_owned()],
            socks: None,
            pac_url: None,
        })),
        ..FetchOptions::default()
    };
    let target = Url::parse(&format!("http://127.0.0.1:{origin_port}/")).expect("target URL");
    let response = fetch(&target, &options).expect("direct fetch bypassing proxy");
    assert_eq!(response.body, b"direct");
    origin.join().expect("origin thread");
}

#[test]
fn resource_loader_uses_the_proxy() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\n\r\n<html></html>"
            .to_vec(),
    ));
    let target = Url::parse("http://origin.test/page").expect("target URL");
    let mut loader = ResourceLoader::with_options(options_with_proxy(proxy_url(proxy.port, "")));
    let resource = loader.load(&target).expect("loader fetch through proxy");
    assert_eq!(resource.bytes, b"<html></html>");
    assert_eq!(resource.content_type, "text/html");
    assert!(request_text(&proxy).starts_with("GET http://origin.test/page HTTP/1.1\r\n"));
}

#[test]
fn from_vars_builds_config_and_matching_rules() {
    let config = ProxyConfig::from_vars(
        Some("http://proxy.example:8080/"),
        Some("https://proxy.example:8443/"),
        Some("example.com, .internal, 127.0.0.1"),
    );
    assert!(config.http.is_some());
    assert!(config.https.is_some());
    let target = Url::parse("http://sub.example.com/").expect("target");
    assert!(config.proxy_for(&target).is_none(), "suffix no_proxy");
    let proxied = Url::parse("http://elsewhere.test/").expect("target");
    assert!(config.proxy_for(&proxied).is_some());
}

#[test]
fn http_proxy_407_response_is_typed_auth_required() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ));
    let target = Url::parse("http://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("407 must be typed");
    assert!(
        matches!(error, FetchError::ProxyAuthRequired(_)),
        "{error:?}"
    );
}

#[test]
fn https_proxy_url_can_forward_http_targets() {
    // TLS proxy: the proxy itself speaks TLS and reads an absolute-form request.
    let certificate = test_certificate(&["localhost"]);
    let (cert, key) = certificate;
    let server_config = Arc::new(
        ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("safe default TLS versions")
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.clone_key())
            .expect("server certificate"),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS proxy");
    let port = listener.local_addr().unwrap().port();
    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_thread = Arc::clone(&captured);
    let handle = thread::spawn(move || {
        let Ok((tcp, _)) = listener.accept() else {
            return;
        };
        let Ok(connection) = rustls::ServerConnection::new(server_config) else {
            return;
        };
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        let request = read_tls_head(&mut stream);
        captured_thread.lock().unwrap().push(request);
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 7\r\n\r\nvia-tls",
        );
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
    });
    let options = FetchOptions {
        connect_timeout: TIMEOUT,
        read_timeout: TIMEOUT,
        tls_config: Some(client_config_trusting(cert)),
        proxy: Some(Arc::new(ProxyConfig {
            http: Some(Url::parse(&format!("https://localhost:{port}/")).expect("https proxy URL")),
            https: None,
            no_proxy: Vec::new(),
            socks: None,
            pac_url: None,
        })),
        ..FetchOptions::default()
    };
    let target = Url::parse("http://127.0.0.1:9/tls-proxied").expect("target URL");
    let response = fetch(&target, &options).expect("http via TLS proxy");
    assert_eq!(response.body, b"via-tls");
    let requests = captured.lock().unwrap();
    let request = String::from_utf8_lossy(&requests[0]).to_string();
    drop(requests);
    assert!(
        request.starts_with("GET http://127.0.0.1:9/tls-proxied HTTP/1.1\r\n"),
        "{request}"
    );
    handle.join().expect("TLS proxy thread");
}

#[test]
fn no_proxy_matches_a_host_with_a_trailing_root_dot() {
    let config = ProxyConfig {
        http: Some(Url::parse("http://proxy.test:8080/").expect("proxy URL")),
        https: None,
        no_proxy: vec!["internal.corp".to_owned()],
        socks: None,
        pac_url: None,
    };
    assert!(config.bypasses("internal.corp.", Some(80)));
    assert!(config.bypasses("sub.internal.corp.", Some(80)));
    assert!(config.bypasses("internal.corp", Some(80)));
    let dotted = Url::parse("http://internal.corp./x").expect("URL");
    assert!(
        config.proxy_for(&dotted).is_none(),
        "the trailing root dot must not send an internal host through the proxy"
    );
}

#[test]
fn no_proxy_matches_dotted_and_bracketed_entries() {
    let config = |entries: Vec<String>| ProxyConfig {
        http: Some(Url::parse("http://proxy.test:8080/").expect("proxy URL")),
        https: None,
        no_proxy: entries,
        socks: None,
        pac_url: None,
    };

    let dotted_entries = config(vec!["internal.corp.".to_owned(), "[::1]".to_owned()]);
    assert!(dotted_entries.bypasses("internal.corp", Some(80)));
    assert!(dotted_entries.bypasses("internal.corp.", Some(80)));
    assert!(dotted_entries.bypasses("sub.internal.corp.", Some(80)));
    assert!(dotted_entries.bypasses("::1", Some(80)));
    assert!(dotted_entries.bypasses("[::1]", Some(80)));
    assert!(!dotted_entries.bypasses("example.com", Some(80)));

    let port_entry = config(vec!["internal.corp.:443".to_owned()]);
    assert!(port_entry.bypasses("internal.corp", Some(443)));
    assert!(!port_entry.bypasses("internal.corp.", Some(80)));

    let wildcard = config(vec!["*".to_owned()]);
    assert!(wildcard.bypasses("example.com", Some(80)));
    assert!(!config(Vec::new()).bypasses("internal.corp.", Some(80)));
}
