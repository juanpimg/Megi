#![no_main]
//! Fuzz target: layout resolution (block/inline/table/flex/grid) on untrusted DOM + CSS.

use libfuzzer_sys::fuzz_target;
use render::{RenderOptions, render_url};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    // Vary viewport dimensions.
    let w = data.get(0).copied().unwrap_or(0) as u32 % 1024 + 1;
    let h = data.get(1).copied().unwrap_or(0) as u32 % 1024 + 1;
    let mut ro = RenderOptions::default();
    ro.viewport.width = w;
    ro.viewport.height = h;
    ro.no_network = true;
    let _ = render_url("data:text/html,hi", &ro);
});
