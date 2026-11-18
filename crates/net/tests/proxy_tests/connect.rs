use crate::*;

#[test]
fn https_via_connect_tunnel_succeeds() {
    let certificate = test_certificate(&["localhost"]);
    let (origin_port, origin) = spawn_tls_origin(
        &certificate,
        b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nsecure".to_vec(),
    );
    let proxy = spawn_proxy(ProxyBehavior::ConnectTunnel(SocketAddr::from((
        [127, 0, 0, 1],
        origin_port,
    ))));
    let target = Url::parse(&format!("https://localhost:{origin_port}/")).expect("target URL");
    let mut options = options_with_proxy(proxy_url(proxy.port, ""));
    options.tls_config = Some(client_config_trusting(certificate.0.clone()));
    let response = fetch(&target, &options).expect("fetch through CONNECT");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"secure");
    let request = request_text(&proxy);
    assert!(
        request.starts_with(&format!("CONNECT localhost:{origin_port} HTTP/1.1\r\n")),
        "CONNECT request line: {request}"
    );
    let _ = origin.join();
}

#[test]
fn connect_407_is_typed_auth_required() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectStatus(
        "HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n",
    ));
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(
        &target,
        &options_with_proxy(proxy_url(proxy.port, "user:pass@")),
    )
    .expect_err("407 must fail");
    assert!(
        matches!(error, FetchError::ProxyAuthRequired(_)),
        "{error:?}"
    );
    let request = request_text(&proxy);
    assert!(
        request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"),
        "CONNECT auth header: {request}"
    );
}

#[test]
fn connect_non_200_is_typed_proxy_connect() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectStatus(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n",
    ));
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("403 CONNECT must fail");
    assert!(matches!(error, FetchError::ProxyConnect(_)), "{error:?}");
}

#[test]
fn broken_tunnel_is_typed_proxy_tunnel() {
    let proxy = spawn_proxy(ProxyBehavior::ConnectThenClose);
    let target = Url::parse("https://example.test/").expect("target URL");
    let error = fetch(&target, &options_with_proxy(proxy_url(proxy.port, "")))
        .expect_err("closed tunnel must fail");
    assert!(matches!(error, FetchError::ProxyTunnel(_)), "{error:?}");
}
