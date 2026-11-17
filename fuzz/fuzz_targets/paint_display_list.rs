#![no_main]
//! Direct fuzz target for `paint` display list, rasterizer, hit-testing, and PNG region encoder (#834, #716).

use css::resolve_document_styles_with_viewport;
use layout::{Rect, layout_document};
use libfuzzer_sys::fuzz_target;
use paint::{
    PngRegionEncoder, build_display_list_and_hit_regions, hit_test, rasterize_display_list,
};

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    let w = u32::from(data[0]).clamp(1, 128);
    let h = u32::from(data[1]).clamp(1, 128);
    let hx = f32::from(data[2]);
    let hy = f32::from(data[3]);
    let rx = f32::from(i8::from_le_bytes([data[4]]));
    let ry = f32::from(i8::from_le_bytes([data[5]]));
    let rw = f32::from(data[6]);
    let rh = f32::from(data[7]);

    let html_src = String::from_utf8_lossy(&data[8..]);
    let parsed = html::parse_document(&html_src, false);
    let styles = resolve_document_styles_with_viewport(&parsed, None, w as f32, h as f32);
    let viewport = Rect::new(0.0, 0.0, w as f32, h as f32);
    if let Some(root) = layout_document(&parsed, &styles, viewport) {
        let (list, regions) = build_display_list_and_hit_regions(&root, viewport);
        let _ = hit_test(&regions, hx, hy);
        let pixmap = rasterize_display_list(&list, w, h);
        let mut enc = PngRegionEncoder::new();
        let _ = enc.encode_regions(&pixmap, &[Rect::new(rx, ry, rw, rh)]);
    }
});
