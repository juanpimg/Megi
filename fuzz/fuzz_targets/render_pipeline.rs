#![no_main]
//! Fuzz target: the full `render` pipeline on untrusted HTML (offline, capped).
//!
//! The input bytes are decoded lossily, base64-encoded into an inline
//! `data:text/html;base64,…` URL (no network access at all) and rendered through
//! [`render::render_url`] with deliberately small caps:
//!
//! - viewport 96×96 (raster + PNG encoding stay cheap per iteration);
//! - `no_network`, scripting disabled (the JS engine has its own target, `js_eval`);
//! - document/CSS/image budgets capped so a single iteration can never allocate GBs.
//!
//! This exercises the integration glue that page content reaches: HTML parse → style
//! collection/cascade → layout with real fonts → display list → hit regions → PNG. The
//! target looks for panics, aborts, ASan findings, integer overflows and timeouts; the
//! render result (Ok/Err) is intentionally discarded.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided document size (the render caps are stricter).
const MAX_FUZZ_INPUT: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let html = String::from_utf8_lossy(data);
    let url = format!("data:text/html;base64,{}", net::base64_encode(html.as_bytes()));

    let Ok(viewport) = render::Viewport::new(96, 96) else {
        return;
    };
    let mut options = render::RenderOptions {
        viewport,
        no_network: true,
        scripting: render::ScriptingMode::Disabled,
        ..render::RenderOptions::default()
    };
    options.max_document_bytes = MAX_FUZZ_INPUT;
    options.max_total_css_bytes = 256 * 1024;
    options.max_css_bytes_per_sheet = 128 * 1024;
    options.max_stylesheets = 4;
    options.max_import_depth = 2;
    options.max_images = 4;
    options.max_image_bytes = 64 * 1024;
    options.max_total_image_bytes = 128 * 1024;
    options.max_backgrounds = 2;
    options.max_background_bytes = 64 * 1024;
    options.max_total_background_bytes = 128 * 1024;

    let _ = render::render_url(&url, &options);
});
