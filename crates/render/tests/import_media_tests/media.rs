//! Media-query evaluation against the viewport.

use super::common::*;

// ---------------------------------------------------------------------------
// media queries
// ---------------------------------------------------------------------------

#[test]
fn link_media_is_evaluated_against_the_viewport() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/wide.css media=\"(min-width: 900px)\">\
             </head><body></body></html>",
        ),
    );
    routes.insert(
        "/wide.css".to_string(),
        RouteResponse::css("html { background-color: rgb(255, 0, 0); }"),
    );
    let wide_server = TestServer::spawn(routes, 2);
    let wide = render_url(&wide_server.url("/page.html"), &options(1024, 768)).expect("wide");
    assert_eq!(wide.stats.media_skipped, 0);
    assert_eq!(wide.pixmap.pixel(5, 5).r, 255);
    wide_server.join();

    let narrow_source = "<!doctype html><html><head>\
        <link rel=stylesheet href=/wide.css media=\"(min-width: 900px)\">\
        </head><body></body></html>";
    let mut routes = HashMap::new();
    routes.insert("/page.html".to_string(), RouteResponse::html(narrow_source));
    let narrow_server = TestServer::spawn(routes, 1);
    let narrow = render_url(&narrow_server.url("/page.html"), &options(600, 800)).expect("narrow");
    assert_eq!(narrow.stats.media_skipped, 1);
    assert_eq!(narrow.pixmap.pixel(5, 5).r, 255);
    assert_eq!(narrow.pixmap.pixel(5, 5).g, 255);
    narrow_server.join();
}

#[test]
fn media_queries_inside_imported_sheets_are_evaluated() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/theme.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/theme.css".to_string(),
        RouteResponse::css(
            "@media (min-width: 900px) { html { background-color: rgb(16, 24, 32); } }",
        ),
    );
    let server = TestServer::spawn(routes, 3);
    let wide = render_url(&server.url("/page.html"), &options(1024, 768)).expect("wide");
    let applied = count_colored(&wide, |pixel| {
        pixel.r == 16 && pixel.g == 24 && pixel.b == 32
    });
    assert!(applied > 0, "imported @media rule must apply at 1024px");
    server.join();
}

#[test]
fn print_media_link_is_skipped_on_screen() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head>\
             <link rel=stylesheet href=/print.css media=print></head><body></body></html>",
        ),
    );
    // No route for /print.css: it must not be fetched.
    let server = TestServer::spawn(routes, 1);
    let output = render_url(&server.url("/page.html"), &options(1024, 768)).expect("render");
    assert_eq!(output.stats.media_skipped, 1);
    assert_eq!(output.pixmap.pixel(5, 5).r, 255);
    server.join();
}
