//! Transparent gzip/deflate decoding end to end.

use super::common::*;

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

#[test]
fn gzip_document_and_stylesheet_are_decoded_end_to_end() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                 </head><body>compressed</body></html>";
    let mut routes = HashMap::new();
    routes.insert(
        "/".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/html; charset=utf-8"),
                ("Content-Encoding", "gzip"),
            ],
            gzip(page.as_bytes()),
        ),
    );
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css"), ("Content-Encoding", "gzip")],
            gzip(b"body { background-color: rgb(255, 0, 0); }"),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let output = render_url(&base, &options(120, 80)).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), RED, "gzip CSS was not applied");
    assert_eq!(output.stats.content_encoding.as_deref(), Some("gzip"));
    assert!(
        output.stats.warnings.is_empty(),
        "unexpected warnings: {:?}",
        output.stats.warnings
    );
    let requests = server.join().expect("server thread");
    assert_eq!(requests.len(), 2);
    assert!(
        header_value(&requests[1], "Accept-Encoding").is_some_and(|value| value.contains("gzip")),
        "stylesheet request lacks Accept-Encoding: {}",
        requests[1]
    );
}

#[test]
fn deflate_stylesheet_is_decoded() {
    let page = "<!DOCTYPE html><html><head><link rel=stylesheet href=/style.css>\
                 </head><body>deflate</body></html>";
    let mut routes = HashMap::new();
    routes.insert("/".to_string(), html(page));
    routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Content-Encoding", "deflate"),
            ],
            zlib(b"body { background-color: rgb(0, 0, 255); }"),
        ),
    );
    let (base, server) = spawn_router(routes, 2);
    let output = render_url(&base, &options(120, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        BLUE,
        "deflate CSS was not applied"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = server.join();
}
