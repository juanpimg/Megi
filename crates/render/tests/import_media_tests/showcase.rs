//! The showcase fixture rendered byte-for-byte to the golden PNG.

use super::common::*;

// ---------------------------------------------------------------------------
// Showcase + golden
// ---------------------------------------------------------------------------

fn showcase_routes() -> HashMap<String, RouteResponse> {
    let root = fixture_dir().join("showcase");
    let mut routes = HashMap::new();
    let mut add = |path: &str, file: &str, content_type: &str| {
        let bytes = std::fs::read(root.join(file)).expect("read fixture");
        routes.insert(
            format!("/showcase/{path}"),
            RouteResponse::new(content_type, bytes),
        );
    };
    add("index.html", "index.html", "text/html; charset=utf-8");
    add("css/site.css", "css/site.css", "text/css");
    add("css/theme.css", "css/theme.css", "text/css");
    add("assets/logo.png", "assets/logo.png", "image/png");
    add("assets/tile.png", "assets/tile.png", "image/png");
    // WP-5.9 CSS backgrounds: the stylesheet declares `../assets/pattern.png` and
    // `../assets/bullet.png`, which resolve against the document base
    // (`/showcase/index.html`) to `/assets/...`.
    let shared_assets = fixture_dir().join("assets");
    routes.insert(
        "/assets/pattern.png".to_string(),
        RouteResponse::new(
            "image/png",
            std::fs::read(shared_assets.join("pattern.png")).expect("pattern asset"),
        ),
    );
    routes.insert(
        "/assets/bullet.png".to_string(),
        RouteResponse::new(
            "image/png",
            std::fs::read(shared_assets.join("bullet.png")).expect("bullet asset"),
        ),
    );
    routes
}

fn golden_png() -> Vec<u8> {
    std::fs::read(fixture_dir().join("showcase/golden.png"))
        .expect("showcase golden.png must exist (regenerate with the documented command)")
}

#[test]
fn showcase_renders_file_url_to_the_golden_png() {
    let output = render_url(&showcase_url(), &options(1024, 768)).expect("showcase render");
    assert_eq!(output.stats.imports_loaded, 1, "theme.css is imported once");
    assert_eq!(output.stats.max_import_depth, 1);
    assert_eq!(output.stats.external_stylesheets, 1, "site.css");
    assert!(output.stats.media_skipped >= 1, "print sheet is skipped");
    assert_eq!(output.stats.images_loaded, 2, "logo + tile");
    assert_eq!(
        output.stats.background_images_loaded, 2,
        "pattern + list-style-image bullet"
    );
    assert_eq!(output.stats.limit_violations, Default::default());
    let golden = golden_png();
    assert_eq!(
        output.png, golden,
        "showcase render differs from tests/fixtures/render/showcase/golden.png; \
         regenerate it with: cargo run -p megi-render -- tests/fixtures/render/showcase/index.html \
         tests/fixtures/render/showcase/golden.png --width 1024 --height 768"
    );
}

#[test]
fn showcase_renders_from_http_to_the_same_golden_png() {
    let server = TestServer::spawn(showcase_routes(), 7);
    let output = render_url(&server.url("/showcase/index.html"), &options(1024, 768))
        .expect("http showcase render");
    assert_eq!(output.stats.imports_loaded, 1);
    assert_eq!(output.stats.images_loaded, 2);
    assert_eq!(output.stats.background_images_loaded, 2);
    assert_eq!(
        output.png,
        golden_png(),
        "http and file:// renders must match"
    );
    server.join();
}

#[test]
fn showcase_render_is_deterministic() {
    let first = render_url(&showcase_url(), &options(1024, 768)).expect("first");
    let second = render_url(&showcase_url(), &options(1024, 768)).expect("second");
    assert_eq!(first.png, second.png);
    assert_eq!(first.stats.display_items, second.stats.display_items);
}
