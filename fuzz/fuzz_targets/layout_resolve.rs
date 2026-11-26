#![no_main]
//! Fuzz target: CSS length/constraint resolution and layout on untrusted DOM + CSS.
//!
//! Calls `megi_css` + `megi_layout` directly (#2420): paint, raster, PNG encoding, JS and
//! the render pipeline are not reachable, so a crash is attributable to style/layout alone.
//! Input, viewport and generated CSS are all tightly bounded.

use libfuzzer_sys::fuzz_target;
use megi_css::resolve_document_styles_with_viewport;
use megi_layout::{Rect, layout_document};

/// Tight input cap: pathological selector/style combinations are not linear in input bytes.
const MAX_FUZZ_INPUT: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 || data.len() > MAX_FUZZ_INPUT {
        return;
    }

    // 32..=256 viewport instead of the previous 1..=1023² production-like render (#2420).
    let vw = 32.0 + f32::from(data[0] % 225);
    let vh = 32.0 + f32::from(data[1] % 225);

    let text = String::from_utf8_lossy(&data[2..]);
    let (html_src, css_src) = match text.split_once("\n---\n") {
        Some((html, css)) => (html, Some(css)),
        None => (text.as_ref(), None),
    };

    // Deterministic calc()/clamp() rule derived from input bytes reaches length and
    // constraint resolution even when the supplied CSS contains no math function (#2420).
    let a = u32::from(data[2]) % 256;
    let b = u32::from(data[3]) % 101;
    let generated_css = format!(
        "* {{ width: calc({a}px + {b}%); max-height: clamp(0px, {b}px, {a}px); }}"
    );
    let css = match css_src {
        Some(css) => format!("{css}\n{generated_css}"),
        None => generated_css,
    };

    let parsed = megi_html::parse_document(html_src, false);
    let styles = resolve_document_styles_with_viewport(&parsed, Some(css.as_str()), vw, vh);
    if let Some(root) = layout_document(&parsed, &styles, Rect::new(0.0, 0.0, vw, vh)) {
        let _ = root.dimensions.content.width;
        let _ = root.dimensions.content.height;
    }
});
