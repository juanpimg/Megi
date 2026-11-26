//! SVG favicon rasterization through the webview facade (WP-USER-ui-favicon).

#[test]
fn svg_favicon_decodes_to_rgba() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ff0000"/></svg>"##;
    let favicon = megi_browser::favicon::Favicon::from_bytes(svg, "https://es.wikipedia.org");
    let pixels = favicon
        .pixels
        .expect("SVG favicon must not fall back to a dot");
    assert!(pixels.iter().any(|&byte| byte != 0));
}

#[test]
fn non_svg_favicon_still_decodes() {
    let png = include_bytes!("../../image/tests/fixtures/png/rgba8.png");
    let favicon = megi_browser::favicon::Favicon::from_bytes(png, "https://example.test");
    assert!(favicon.pixels.is_some(), "known PNG must still decode");
}

#[test]
fn oversized_svg_favicon_is_rejected() {
    let bytes = vec![b'x'; 1_000_000];
    assert!(megi_webview::render::decode_image_with_svg(&bytes, 16).is_none());
}
