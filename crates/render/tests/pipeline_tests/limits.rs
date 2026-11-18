//! Stylesheet caps and non-fatal warning behaviour.

use super::common::*;

#[test]
fn max_external_stylesheets_is_enforced() {
    let mut routes = HashMap::new();
    routes.insert(
        "/many.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/a.css><link rel=stylesheet href=/b.css>\
             <link rel=stylesheet href=/c.css></head><body></body></html>",
        ),
    );
    routes.insert("/a.css".to_string(), RouteResponse::css("p{color:red}"));
    routes.insert("/b.css".to_string(), RouteResponse::css("p{color:green}"));
    let server = TestServer::spawn(routes, 3);
    let mut options = options(160, 80);
    options.max_stylesheets = 2;
    let output = render_url(&server.url("/many.html"), &options).expect("render with sheet cap");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("stylesheet cap reached")),
        "expected a stylesheet-cap warning, got {:?}",
        output.stats.warnings
    );
    server.join();
}

#[test]
fn oversized_stylesheet_is_rejected() {
    let mut routes = HashMap::new();
    routes.insert(
        "/big.html".to_string(),
        RouteResponse::html("<!doctype html><html><head><link rel=stylesheet href=/big.css></head><body></body></html>"),
    );
    routes.insert(
        "/big.css".to_string(),
        RouteResponse::css(&"p{color:red}".repeat(200)),
    );
    let server = TestServer::spawn(routes, 2);
    let mut options = options(120, 60);
    options.max_css_bytes_per_sheet = 64;
    let output =
        render_url(&server.url("/big.html"), &options).expect("render with oversized sheet");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("exceeds the 64 byte cap")),
        "expected an oversized-sheet warning, got {:?}",
        output.stats.warnings
    );
    server.join();
}

#[test]
fn failing_stylesheet_is_a_warning_not_a_fatal_error() {
    let port = closed_port();
    let html = format!(
        "<!doctype html><html><head><link rel=stylesheet href=\"http://127.0.0.1:{port}/missing.css\"></head>\
         <body><p>still renders</p></body></html>"
    );
    let mut routes = HashMap::new();
    routes.insert("/page.html".to_string(), RouteResponse::html(&html));
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(
        output.stats.warnings.len(),
        1,
        "{:?}",
        output.stats.warnings
    );
    server.join();
}
