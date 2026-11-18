//! End-to-end render tests for filter and backdrop-filter fixtures (#1437).

use std::path::Path;

use render::{RenderOptions, Viewport, render_url};

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/render")
}

fn fixture_url(name: &str) -> String {
    format!("file://{}", fixture_dir().join(name).display())
}

fn near(a: u8, b: u8, tolerance: u8) -> bool {
    a.abs_diff(b) <= tolerance
}

fn pixel(output: &render::RenderOutput, x: u32, y: u32) -> css::Color {
    let idx = (y as usize) * (output.pixmap.width as usize) + (x as usize);
    output.pixmap.pixels[idx]
}

#[test]
fn filter_effects_fixture_renders_expected_filtered_colors() {
    let options = RenderOptions {
        viewport: Viewport::new(420, 110).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&fixture_url("filter_effects.html"), &options).expect("render");

    // Row 1, box 1: grayscale(red) -> dark gray.
    let gray = pixel(&output, 36, 24);
    assert!(
        near(gray.r, gray.g, 6) && near(gray.g, gray.b, 6),
        "grayscale: {gray:?}"
    );
    assert!(near(gray.r, 54, 12), "grayscale luma: {gray:?}");

    // Row 1, box 4: invert(red) -> cyan.
    let invert = pixel(&output, 222, 24);
    assert!(
        invert.r < 20 && invert.g > 235 && invert.b > 235,
        "invert(red): {invert:?}"
    );

    // Row 1, box 5: opacity(50%) red over white -> pink.
    let opacity = pixel(&output, 284, 24);
    assert!(
        opacity.r > 240 && near(opacity.g, 128, 12) && near(opacity.b, 128, 12),
        "opacity(50%): {opacity:?}"
    );

    // Row 1, box 6: sepia(red) -> warm brownish tone (~100,89,69).
    let sepia = pixel(&output, 346, 24);
    assert!(
        sepia.r > sepia.g && sepia.g > sepia.b && near(sepia.r, 100, 12) && near(sepia.g, 89, 12),
        "sepia(red): {sepia:?}"
    );
}

#[test]
fn backdrop_filter_fixture_renders_overlays() {
    let options = RenderOptions {
        viewport: Viewport::new(260, 150).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&fixture_url("backdrop_filter.html"), &options).expect("render");

    // Plain solid overlay remains #336699.
    let plain = pixel(&output, 60, 96);
    assert_eq!(
        (plain.r, plain.g, plain.b),
        (0x33, 0x66, 0x99),
        "plain overlay: {plain:?}"
    );

    // Dim/backdrop-filter overlays should affect pixels (not plain background color).
    let dim = pixel(&output, 165, 40);
    assert!(
        dim != plain,
        "dim backdrop-filter overlay should differ from plain overlay: {dim:?}"
    );
}

#[test]
fn animation_time_fixture_renders_at_requested_time() {
    let dir = fixture_dir();
    let url = format!("file://{}", dir.join("animation_time.html").display());

    let render_at = |time_ms: f64| {
        let options = RenderOptions {
            viewport: Viewport::new(100, 100).expect("viewport"),
            time_ms,
            ..RenderOptions::default()
        };
        render_url(&url, &options).expect("render")
    };

    let start = pixel(&render_at(0.0), 50, 50);
    assert!(
        start.r > 240 && start.g > 240 && start.b > 240,
        "opacity 0 at t=0 shows white: {start:?}"
    );

    let middle = pixel(&render_at(500.0), 50, 50);
    assert!(
        middle.r > 240 && (80..=180).contains(&middle.g) && (80..=180).contains(&middle.b),
        "opacity 0.5 at t=500 blends red over white: {middle:?}"
    );

    let end = pixel(&render_at(1000.0), 50, 50);
    assert!(
        end.r > 240 && end.g < 10 && end.b < 10,
        "opacity 1 at t=1000 is opaque red: {end:?}"
    );
}
