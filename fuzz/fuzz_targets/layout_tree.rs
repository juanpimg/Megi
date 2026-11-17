#![no_main]
//! Direct fuzz target for `layout::layout_document` (#834).

use css::resolve_document_styles_with_viewport;
use layout::{Rect, layout_document};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let vw = (u16::from_le_bytes([data[0], data[1]]) % 2048).max(1) as f32;
    let vh = (u16::from_le_bytes([data[2], data[3]]) % 2048).max(1) as f32;
    let text = String::from_utf8_lossy(&data[4..]);
    let (html_src, css_src) = match text.split_once("\n---\n") {
        Some((h, c)) => (h, Some(c)),
        None => (text.as_ref(), None),
    };
    let parsed = html::parse_document(html_src, false);
    let styles = resolve_document_styles_with_viewport(&parsed, css_src, vw, vh);
    if let Some(root) = layout_document(&parsed, &styles, Rect::new(0.0, 0.0, vw, vh)) {
        let _ = root.dimensions.content.width;
    }
});
