//! CORS handling for cross-origin subresources.

use super::common::*;

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

#[test]
fn cross_origin_stylesheet_sends_document_origin_and_applies_when_allowed() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", "*"),
            ],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet crossorigin href=\"{css_base}/style.css\">\
         </head><body>cors</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);

    let output = render_url(&html_base, &options(120, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        RED,
        "CORS-allowed stylesheet skipped"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );

    let html_requests = html_server.join().expect("html server");
    let css_requests = css_server.join().expect("css server");
    let document_origin =
        header_value(&css_requests[0], "Origin").expect("stylesheet request with Origin?");
    let expected = megi_net::Origin::parse(&html_base)
        .expect("html origin")
        .as_str();
    assert_eq!(document_origin, expected);
    assert!(header_value(&html_requests[0], "Origin").is_none());
}

#[test]
fn cross_origin_stylesheet_without_acao_is_a_non_fatal_warning() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![("Content-Type", "text/css")],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet crossorigin href=\"{css_base}/style.css\">\
         </head><body>denied</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let output = render_url(&html_base, &options(120, 80)).expect("render survives CORS denial");
    assert_ne!(
        output.pixmap.pixel(5, 5),
        RED,
        "denied stylesheet must not apply"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("CORS") && warning.contains("style.css")),
        "missing non-fatal CORS warning: {:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let _ = css_server.join();
}

#[test]
fn credentialed_wildcard_cors_is_rejected_as_warning() {
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", "*"),
                ("Access-Control-Allow-Credentials", "true"),
            ],
            b"body { background-color: rgb(255, 0, 0); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet crossorigin href=\"{css_base}/style.css\">\
         </head><body>credentials</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let mut render_options = options(120, 80);
    render_options.cookies = true;
    let output = render_url(&html_base, &render_options).expect("render");
    assert_ne!(output.pixmap.pixel(5, 5), RED);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("credentials")),
        "missing credentials CORS warning: {:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let _ = css_server.join();
}

#[test]
fn explicit_origin_override_is_used_for_subresources() {
    let override_origin = "https://app.example";
    let mut css_routes = HashMap::new();
    css_routes.insert(
        "/style.css".to_string(),
        route(
            200,
            vec![
                ("Content-Type", "text/css"),
                ("Access-Control-Allow-Origin", override_origin),
            ],
            b"body { background-color: rgb(0, 0, 255); }".to_vec(),
        ),
    );
    let (css_base, css_server) = spawn_router(css_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><head><link rel=stylesheet crossorigin href=\"{css_base}/style.css\">\
         </head><body>override</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);
    let mut render_options = options(120, 80);
    render_options.origin = Some(override_origin.to_string());
    let output = render_url(&html_base, &render_options).expect("render");
    assert_eq!(output.pixmap.pixel(5, 5), BLUE);
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let css_requests = css_server.join().expect("css server");
    assert_eq!(
        header_value(&css_requests[0], "Origin").as_deref(),
        Some(override_origin)
    );
}
