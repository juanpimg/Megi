//! `no-cors` subresources: cross-origin requests that skip CORS entirely.

use super::common::*;

#[test]
fn no_cors_cross_origin_stylesheet_without_acao_applies_and_omits_origin() {
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
        "<!DOCTYPE html><html><head><link rel=stylesheet href=\"{css_base}/style.css\">\
         </head><body>no-cors-css</body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);

    let output = render_url(&html_base, &options(120, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        RED,
        "no-cors cross-origin stylesheet without ACAO should apply (#1537)"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let css_requests = css_server.join().expect("css server");
    assert!(
        header_value(&css_requests[0], "Origin").is_none(),
        "no-cors stylesheet request must not send Origin"
    );
}

#[test]
fn no_cors_cross_origin_background_image_loads_without_acao() {
    let mut img_routes = HashMap::new();
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60"><rect width="60" height="60" fill="rgb(0,0,255)"/></svg>"#;
    img_routes.insert(
        "/bg.svg".to_string(),
        route(200, vec![("Content-Type", "image/svg+xml")], svg.to_vec()),
    );
    let (img_base, img_server) = spawn_router(img_routes, 1);
    let page = format!(
        "<!DOCTYPE html><html><body style=\"margin:0; width:60px; height:60px; background-image:url('{img_base}/bg.svg')\"></body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);

    let output = render_url(&html_base, &options(80, 80)).expect("render");
    assert_eq!(
        output.pixmap.pixel(10, 10),
        BLUE,
        "no-cors CSS background-image without ACAO should paint (#1537)"
    );
    assert!(
        output.stats.warnings.is_empty(),
        "{:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let img_requests = img_server.join().expect("img server");
    assert!(
        header_value(&img_requests[0], "Origin").is_none(),
        "no-cors CSS background request must not send Origin"
    );
}

#[cfg(feature = "js")]
#[test]
fn no_cors_classic_script_runs_without_acao_while_crossorigin_requires_cors() {
    let mut js_routes = HashMap::new();
    js_routes.insert(
        "/no-cors.js".to_string(),
        route(
            200,
            vec![("Content-Type", "application/javascript")],
            b"document.body.setAttribute('style', 'background-color: rgb(0, 0, 255)');".to_vec(),
        ),
    );
    js_routes.insert(
        "/cors-denied.js".to_string(),
        route(
            200,
            vec![("Content-Type", "application/javascript")],
            b"document.body.setAttribute('style', 'background-color: rgb(255, 0, 0)');".to_vec(),
        ),
    );
    let (js_base, js_server) = spawn_router(js_routes, 2);
    let page = format!(
        "<!DOCTYPE html><html><body>hello\
         <script src=\"{js_base}/no-cors.js\"></script>\
         <script src=\"{js_base}/cors-denied.js\" crossorigin></script>\
         </body></html>"
    );
    let mut html_routes = HashMap::new();
    html_routes.insert("/".to_string(), html(&page));
    let (html_base, html_server) = spawn_router(html_routes, 1);

    let mut render_options = options(120, 80);
    render_options.scripting = megi_render::ScriptingMode::Enabled;
    let output = render_url(&html_base, &render_options).expect("render");
    assert_eq!(
        output.pixmap.pixel(5, 5),
        BLUE,
        "no-cors classic script runs; crossorigin script without ACAO is blocked"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("CORS") && warning.contains("cors-denied.js")),
        "missing CORS warning for crossorigin script: {:?}",
        output.stats.warnings
    );
    let _ = html_server.join();
    let js_requests = js_server.join().expect("js server");
    let no_cors_req = js_requests
        .iter()
        .find(|r| r.contains("/no-cors.js"))
        .expect("no-cors request");
    let cors_req = js_requests
        .iter()
        .find(|r| r.contains("/cors-denied.js"))
        .expect("cors request");
    assert!(header_value(no_cors_req, "Origin").is_none());
    assert!(header_value(cors_req, "Origin").is_some());
}
