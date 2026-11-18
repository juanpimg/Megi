use crate::*;

#[test]
fn http_via_proxy_uses_absolute_form_and_target_host() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
    ));
    let target = Url::parse("http://127.0.0.1:9/path?q=1").expect("target URL");
    let response = fetch(&target, &options_with_proxy(proxy_url(proxy.port, ""))).expect("fetch");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
    let request = request_text(&proxy);
    assert!(
        request.starts_with("GET http://127.0.0.1:9/path?q=1 HTTP/1.1\r\n"),
        "absolute-form request line: {request}"
    );
    assert!(request.contains("Host: 127.0.0.1:9\r\n"), "{request}");
    assert_eq!(proxy.hits.load(Ordering::SeqCst), 1);
}

#[test]
fn http_via_proxy_sends_basic_authorization() {
    let proxy = spawn_proxy(ProxyBehavior::Respond(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
    ));
    let target = Url::parse("http://example.test/").expect("target URL");
    let response = fetch(
        &target,
        &options_with_proxy(proxy_url(proxy.port, "user:pass@")),
    )
    .expect("fetch");
    assert_eq!(response.body, b"ok");
    let request = request_text(&proxy);
    assert!(
        request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"),
        "Basic auth header: {request}"
    );
}
