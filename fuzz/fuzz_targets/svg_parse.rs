#![no_main]
//! Fuzz target: SVG parsing + rasterization with strict anti-DoS limits.
//!
//! Arbitrary bytes are fed to [`svg::render_svg_with_limits`] with a small output size
//! (≤128×128) and every [`svg::Limits`] cap tightened (input 64 KiB, depth 32, 512
//! elements, 4k path segments, 128 px dimensions / 16K pixels). This exercises the XML
//! reader, attribute/value parsers (lengths, colours, transforms, path data, gradients,
//! patterns, `<use>` expansion) and the rasterizer while bounded by the same limits a real
//! page hits.
//!
//! The target looks for panics, aborts, ASan/UBSan findings, integer overflows and
//! timeouts; `Ok`/`Err` results are intentionally discarded (typed [`svg::SvgError`]s are
//! expected for malformed input).

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = svg::Limits {
        max_input_bytes: 64 * 1024,
        max_depth: 32,
        max_elements: 512,
        max_attributes: 64,
        max_path_segments: 4_096,
        max_dimension: 128,
        max_height: 128,
        max_pixels: 128 * 128,
        max_use_depth: 4,
        max_selector_checks: 4_096,
    };
    let options = svg::SvgOptions {
        width: Some(64),
        height: Some(64),
    };
    let _ = svg::render_svg_with_limits(data, &options, &limits);
});
