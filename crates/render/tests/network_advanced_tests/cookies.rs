//! Cookie-jar sharing across documents, subresources, redirects and renders.

use super::common::*;

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

#[test]
fn cookies_are_shared_between_document_and_subresources() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                 </head><body>cookies</body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Set-Cookie", "session=abc123; Path=/"),
            ],
            page.as_bytes().to_vec(),
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css")],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&base, &render_options).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), RED, "stylesheet was skipped");
    assert_eq!(output.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("session=abc123")),
        "stylesheet request lacks the document cookie: {}",
        requests[1]
    );
    assert!(
        header_value(&requests[0], "Cookie").is_none(),
        "first request must not carry cookies"
    );
}

#[test]
fn redirect_hops_keep_cookies_for_the_final_request() {
    let mut routes = HashMap::new();
    routes.insert(
        "/start".to_string(),
        route(
            302,
            vec![("Location", "/final"), ("Set-Cookie", "hop=1; Path=/")],
            Vec::new(),
        ),
    );
    routes.insert("/final".to_string(), html("<p>final</p>"));
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&format!("{base}/start"), &render_options).expect("render");
    assert!(output.document_url.as_str().ends_with("/final"));
    assert_eq!(output.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("hop=1")),
        "redirect did not carry the Set-Cookie: {}",
        requests[1]
    );
}

#[test]
fn shared_cookie_jar_persists_across_renders() {
    let jar = Arc::new(Mutex::new(megi_net::CookieJar::new()));
    let mut routes = HashMap::new();
    routes.insert(
        "/first".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Set-Cookie", "persisted=1; Path=/"),
            ],
            b"<p>first</p>".to_vec(),
        ),
    );
    routes.insert("/second".to_string(), html("<p>second</p>"));
    let (base, server) = spawn_router(routes, 2);
    let mut render_options = options(120, 80);
    render_options.cookie_jar = Some(Arc::clone(&jar));
    render_url(&format!("{base}/first"), &render_options).expect("first render");
    let second = render_url(&format!("{base}/second"), &render_options).expect("second render");
    assert_eq!(second.stats.cookies_stored, 1);
    let requests = server.join().expect("server thread");
    assert!(
        header_value(&requests[1], "Cookie").is_some_and(|value| value.contains("persisted=1")),
        "shared jar did not persist the cookie across renders: {}",
        requests[1]
    );
}
