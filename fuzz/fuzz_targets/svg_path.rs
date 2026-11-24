#![no_main]
//! Fuzz target: SVG path-data grammar through the public rasterizer entry points.
//!
//! `svg_parse` feeds raw bytes to the whole SVG document parser; bytes containing XML
//! metacharacters are rejected before the `d` attribute grammar is reached. This target
//! maps input bytes into the SVG path alphabet (commands, separators, signs, digits and
//! exponent markers) and renders a minimal `<path>` document, so both accepted and
//! malformed path data reach `parse_path_data` (SVG 1.1 §9.3). Expected `SvgError`s are
//! ignored; panics and overflows are findings.

use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_INPUT: usize = 64 * 1024;
const MAX_PATH_CHARS: usize = 16 * 1024;
/// All path commands plus separators, signs, digits, exponent markers and one unknown
/// command (`X`/`x`) to exercise the rejection path. Deliberately excludes XML-sensitive
/// bytes (`<`, `&`, `"`) so the generated attribute stays well formed.
const PATH_ALPHABET: &[u8] = b"MmLlHhVvCcSsQqTtAaZzXx+-.0123456789eE ,";

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let mut path = String::with_capacity(data.len().min(MAX_PATH_CHARS) + 1);
    for byte in data.iter().take(MAX_PATH_CHARS) {
        path.push(PATH_ALPHABET[usize::from(*byte) % PATH_ALPHABET.len()] as char);
    }
    if path.is_empty() {
        path.push('M');
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" viewBox=\"0 0 64 64\">\
         <path d=\"{path}\" fill=\"#345\" stroke=\"#000\" stroke-width=\"1\"/></svg>"
    );
    // Parse-only pass reports path errors through `SvgError::Invalid` without rasterizing.
    let limits = megi_svg::Limits {
        max_input_bytes: MAX_FUZZ_INPUT + 4 * 1024,
        max_depth: 16,
        max_elements: 32,
        max_attributes: 16,
        max_path_segments: 4_096,
        max_dimension: 64,
        max_height: 64,
        max_pixels: 64 * 64,
        max_use_depth: 2,
        max_use_expansions: 8,
        max_shapes: 4_096,
        max_selector_checks: 1_024,
    };
    let options = megi_svg::SvgOptions {
        width: Some(64),
        height: Some(64),
    };
    let _ = megi_svg::intrinsic_size_with_limits(svg.as_bytes(), &limits);
    let _ = megi_svg::render_svg_with_limits(svg.as_bytes(), &options, &limits);
});
