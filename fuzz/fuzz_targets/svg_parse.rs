#![no_main]
//! Fuzz target: SVG parsing + rasterization across option and limit profiles.
//!
//! Arbitrary bytes go to [`svg::render_svg_with_limits`]. Earlier revisions pinned one
//! `SvgOptions` (64×64) and one [`svg::Limits`] profile (#705), so neither the option
//! handling (missing/zero/huge sizes, aspect-ratio fitting) nor the *roomier* limits that a
//! real page can hit were fuzzed. This target now drives:
//!
//! - `SvgOptions` sizes derived from the input, plus `None`, `Some(0)` and `Some(huge)`;
//! - a **tight** profile (128 px, 512 elements, 4k segments) and a **roomy** profile
//!   (1024 px, 8k elements, 64k segments) so the caps themselves are exercised on both
//!   sides of the boundary;
//! - `svg::intrinsic_size_with_limits` (parse-only, no rasterization) so the
//!   XML/attribute/length parsers get coverage even when rasterization is rejected by a cap.
//!
//! Signals: panics, aborts, ASan/UBSan findings, integer overflows and timeouts. Typed
//! [`svg::SvgError`]s are expected and ignored.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input.
const MAX_FUZZ_INPUT: usize = 256 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let at = |index: usize| -> u32 { u32::from(data.get(index).copied().unwrap_or(0)) };

    let tight = svg::Limits {
        max_input_bytes: 64 * 1024,
        max_depth: 32,
        max_elements: 512,
        max_attributes: 64,
        max_path_segments: 4_096,
        max_dimension: 128,
        max_height: 128,
        max_pixels: 128 * 128,
        max_use_depth: 4,
        max_use_expansions: 16,
        max_shapes: 4_096,
        max_selector_checks: 4_096,
    };
    // Roomier profile: same order of magnitude as a page-embedded SVG, still bounded.
    let roomy = svg::Limits {
        max_input_bytes: MAX_FUZZ_INPUT,
        max_depth: 64,
        max_elements: 8_192,
        max_attributes: 256,
        max_path_segments: 65_536,
        max_dimension: 1_024,
        max_height: 1_024,
        max_pixels: 1_024 * 1_024,
        max_use_depth: 12,
        max_use_expansions: 256,
        max_shapes: 65_536,
        max_selector_checks: 65_536,
    };

    // Option space: absent, zero, input-derived and absurd sizes.
    let options = [
        svg::SvgOptions {
            width: None,
            height: None,
        },
        svg::SvgOptions {
            width: Some(0),
            height: Some(0),
        },
        svg::SvgOptions {
            width: Some(at(0) % 1_024),
            height: Some(at(1) % 1_024),
        },
        svg::SvgOptions {
            width: Some(u32::MAX),
            height: Some(at(2) % 4),
        },
    ];

    // Parse/intrinsic-size pass: the XML/attribute/length parsers without rasterization.
    let _ = svg::intrinsic_size_with_limits(data, &tight);

    for limits in [&tight, &roomy] {
        for options in &options {
            let _ = svg::render_svg_with_limits(data, options, limits);
        }
    }
    // Same parse pass against the roomy profile.
    let _ = svg::intrinsic_size_with_limits(data, &roomy);
});
