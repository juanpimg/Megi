//! `@import` cascade, media gating, cycle detection and depth limits.

use super::common::*;

// ---------------------------------------------------------------------------
// @import
// ---------------------------------------------------------------------------

#[test]
fn import_is_cascaded_before_the_importing_sheet_rules() {
    // b.css is imported first (green p), then a.css's own rule (red p) wins by source order.
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head>\
             <body><p class=msg>order</p></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\np.msg { color: rgb(255, 0, 0); }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("p.msg { color: rgb(0, 128, 0); }"),
    );
    let server = TestServer::spawn(routes, 3);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.external_stylesheets, 1);
    let red = count_colored(&output, |pixel| {
        pixel.r > 180 && pixel.g < 80 && pixel.b < 80
    });
    let green = count_colored(&output, |pixel| {
        pixel.g > 90 && pixel.r < 80 && pixel.b < 80
    });
    assert!(
        red > 0,
        "importer rule (red) must win: red={red} green={green}"
    );
    assert!(
        green == 0,
        "imported rule must be overridden: green={green}"
    );
    server.join();
}

#[test]
fn import_media_gate_avoids_the_fetch() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\") print;\nbody { background: rgb(0, 0, 255); }"),
    );
    // No route for /b.css: the media gate must skip it before fetching.
    let server = TestServer::spawn(routes, 2);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 0);
    assert!(output.stats.imports_skipped >= 1);
    assert!(server.requests().iter().all(|path| path != "/b.css"));
    server.join();
}

#[test]
fn import_cycles_are_detected_and_deduplicated() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\np { color: red; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("@import url(\"/a.css\");\np { color: green; }"),
    );
    let server = TestServer::spawn(routes, 3);
    let output = render_url(&server.url("/page.html"), &options(200, 100)).expect("render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert!(output.stats.imports_skipped >= 1);
    server.join();
}

#[test]
fn import_depth_limit_skips_deeper_sheets() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css("@import url(\"/c.css\");\np { color: green; }"),
    );
    // /c.css is never requested because of the depth limit.
    let server = TestServer::spawn(routes, 3);
    let mut options = options(200, 100);
    options.max_import_depth = 1;
    let output = render_url(&server.url("/page.html"), &options).expect("render");
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.imports_loaded, 1);
    assert!(output.stats.imports_skipped >= 1);
    assert!(server.requests().iter().all(|path| path != "/c.css"));
    server.join();
}

#[test]
fn total_css_budget_degrades_to_a_warning_on_imported_sheets() {
    let mut routes = HashMap::new();
    routes.insert(
        "/page.html".to_string(),
        RouteResponse::html(
            "<!doctype html><html><head><link rel=stylesheet href=/a.css></head><body></body></html>",
        ),
    );
    routes.insert(
        "/a.css".to_string(),
        RouteResponse::css("@import url(\"/b.css\");\nbody { background: white; }"),
    );
    routes.insert(
        "/b.css".to_string(),
        RouteResponse::css(&"p { color: red; }\n".repeat(200)),
    );
    let server = TestServer::spawn(routes, 3);
    let mut options = options(200, 100);
    options.max_total_css_bytes = 128;
    // #654: exceeding the byte budget skips the offending sheet with a warning; the render
    // still produces output.
    let output = render_url(&server.url("/page.html"), &options).expect("render must degrade");
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("CSS byte budget")),
        "expected a byte-budget warning, got {:?}",
        output.stats.warnings
    );
    assert!(output.stats.imports_skipped >= 1);
    server.join();
}
