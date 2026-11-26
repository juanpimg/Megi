//! Hacker News visual-parity regression (WP-USER-ui-gate / auto-round-55-hn-mae).
//!
//! The window-UI gate measures the mean absolute channel difference (MAE) between a
//! Megi screenshot and a real Chromium reference of the same saved HN snapshot. The
//! gate failed at 21.810 against a limit of 18: the saved HN page is a legacy
//! `cellpadding="0" cellspacing="0"` table whose default UA typography Chrome renders
//! on Times New Roman 16px. Megi's bundled sans face is wider and its UA sheet did
//! not honour the legacy presentational hints, so the whole story list was larger
//! and shifted. This test reproduces the gate measurement end-to-end (HTML → layout →
//! raster → PNG) at the same 1000x700 viewport and asserts the approved limit.
//!
//! WHATWG HTML §15.2 (presentational hints) and §15.3.8 (table rendering).

use std::path::Path;

use megi_image::{PngLimits, RgbaImage, decode_png};
use megi_render::{RenderOptions, Viewport, render_url};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Mean absolute RGB-channel difference, matching `tools/check_window_ui.py`.
fn mae(a: &RgbaImage, b: &RgbaImage) -> f64 {
    assert_eq!(
        (a.width, a.height),
        (b.width, b.height),
        "screenshot and reference must have equal dimensions"
    );
    let mut total = 0u64;
    let left_pixels = a.data.as_chunks::<4>().0;
    let right_pixels = b.data.as_chunks::<4>().0;
    for (left, right) in left_pixels.iter().zip(right_pixels.iter()) {
        for channel in 0..3 {
            total += left[channel].abs_diff(right[channel]) as u64;
        }
    }
    total as f64 / (a.width as f64 * a.height as f64 * 3.0)
}

fn render_hn_snapshot() -> RgbaImage {
    let url = format!("file://{}", fixture("real-sites/hn.html").display());
    let options = RenderOptions {
        viewport: Viewport::new(1000, 700).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render HN snapshot");
    decode_png(&output.png, &PngLimits::default()).expect("decode HN screenshot")
}

#[test]
fn hn_snapshot_visual_parity_is_within_the_gate_limit() {
    // The real Chromium 1000x700 capture of the fixture, versioned here as a golden
    // so the regression is reproducible in CI without the untracked chrome/ui refs.
    let reference = decode_png(
        include_bytes!("golden/hn_chrome_reference.png"),
        &PngLimits::default(),
    )
    .expect("decode Chromium reference");
    let screenshot = render_hn_snapshot();
    let value = mae(&screenshot, &reference);
    assert!(
        value <= 18.0,
        "HN MAE {value:.3} exceeds the gate limit 18.0; \
         the legacy table typography/presentational hints regressed"
    );
}
