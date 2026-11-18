//! HTTP document and stylesheet rendering, redirects and error statuses.

use super::common::*;

#[test]
fn http_page_with_external_css_renders_the_css() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/style.css\"></head>\
             <body><p>external CSS</p></body></html>",
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        RouteResponse::css("html { background-color: rgb(255, 0, 0); }"),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.status, Some(200));
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(output.stats.inline_stylesheets, 0);
    assert!(output.stats.css_bytes > 0);
    assert!(
        pixel_is(&output.pixmap, 5, 5, 255, 0, 0),
        "the external stylesheet must paint the canvas red, got {:?}",
        output.pixmap.pixel(5, 5)
    );
    server.join();
}

#[test]
fn http_external_css_with_utf8_bom_applies_first_rule() {
    // A leading UTF-8 BOM is a valid non-ASCII ident start in CSS; without stripping it
    // (#1449) the first selector becomes "\u{FEFF}html" and matches nothing, so the reset
    // rule is silently lost. Serve raw bytes so the BOM reaches the stylesheet decoder.
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/bom.css\"></head>\
             <body><p>bom</p></body></html>",
        ),
    );
    routes.insert(
        "/bom.css".to_string(),
        RouteResponse {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/css".to_string())],
            body: b"\xEF\xBB\xBFhtml { background-color: rgb(0, 128, 0); }".to_vec(),
        },
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.external_stylesheets, 1);
    assert!(
        pixel_is(&output.pixmap, 5, 5, 0, 128, 0),
        "a UTF-8 BOM must not swallow the first CSS rule, got {:?}",
        output.pixmap.pixel(5, 5)
    );
    server.join();
}

#[test]
fn http_page_without_external_css_stays_white() {
    let mut routes = HashMap::new();
    routes.insert(
        "/plain.html".to_string(),
        RouteResponse::html("<!doctype html><html><body><p>no css</p></body></html>"),
    );
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/plain.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.external_stylesheets, 0);
    assert!(
        pixel_is(&output.pixmap, 5, 5, 255, 255, 255),
        "without CSS the canvas stays white"
    );
    server.join();
}

#[test]
fn http_error_status_is_a_typed_error() {
    let mut routes = HashMap::new();
    routes.insert("/missing.html".to_string(), RouteResponse::status(404));
    let server = TestServer::spawn(routes, 1);
    let error = render_url(&server.url("/missing.html"), &options(100, 100))
        .expect_err("404 document must fail");
    assert!(
        matches!(error, RenderError::HttpStatus { status: 404, .. }),
        "{error:?}"
    );
    server.join();
}

#[test]
fn redirects_are_followed_and_final_url_is_reported() {
    let mut routes = HashMap::new();
    routes.insert("/go".to_string(), RouteResponse::redirect("/page.html"));
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html("<!doctype html><html><body><p>redirected</p></body></html>"),
    );
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/go"), &options(200, 100)).expect("redirect render");
    assert_eq!(output.stats.status, Some(200));
    assert_eq!(output.document_url.path(), "/page.html");
    server.join();
}
