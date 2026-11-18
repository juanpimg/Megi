//! Benchmark registry: every measured workload lives here.
//!
//! Fixtures are generated and parsed in [`register`], *before* the timed loop;
//! only the operation under test runs inside the measurement window. The
//! division mirrors `docs/PERFORMANCE.md` and the row names are a stable API
//! for regression tracking.

mod backdrop;
mod effects;
#[cfg(feature = "fonts")]
mod fonts;
mod html;
mod image;
mod layout;
mod png;

use backdrop::{
    register_backdrop_blur_rows, register_backdrop_stage_rows, register_effect_fixture_pipelines,
    register_png_region_encoders,
};
use effects::{register_fixture_effect_rows, register_layer_effects};
#[cfg(feature = "fonts")]
use fonts::register_fonts;
use html::{register_css, register_html};
use image::{register_image, register_render_pipeline};
use layout::{register_feature_e2e, register_layout_and_paint};
use png::register_png_rows;

use std::hint::black_box;
use std::sync::Arc;

use megi_core::{Document, NodeId};
use megi_html::ParseOptions;
use megi_layout::Rect;

use crate::cli::Options;
use crate::generators;
use crate::runner::{BenchEntry, Suite};

/// Parse limits used to measure the *algorithmic* cost of extreme inputs.
///
/// The production defaults (`max_depth: 4096`, `max_attributes_per_element:
/// 4096`) intentionally truncate hostile input; measuring them would only show
/// the cost of the guard, not of the parser. These benchmarks therefore lift
/// every limit so the reported time is the parser's real work on the full
/// input. `html/nested-div-64k-default-limits` keeps the defaults on purpose
/// and documents the anti-DoS behaviour.
const GENEROUS_PARSE: ParseOptions = ParseOptions {
    max_depth: usize::MAX,
    max_nodes: usize::MAX,
    max_attributes_per_element: usize::MAX,
    max_input_bytes: usize::MAX,
    max_text_bytes: usize::MAX,
    max_written_bytes: usize::MAX,
    max_script_executions: usize::MAX,
    max_script_text_bytes: usize::MAX,
    max_active_formatting: usize::MAX,
};

const VIEWPORT_WIDTH: u32 = 800;
const VIEWPORT_HEIGHT: u32 = 600;

/// Committed 800×600 PNG fixture from `crates/image` (13 947 B, RGBA8).
static PNG_800X600: &[u8] = include_bytes!("../../../image/tests/fixtures/png/large_800x600.png");
/// Deterministic baseline JPEG (800×600, quality 85, ~75 KiB). See
/// `crates/bench/fixtures/README.md` for the exact generation recipe.
static JPEG_800X600: &[u8] = include_bytes!("../../fixtures/jpeg-800x600.jpg");
/// Deterministic indexed GIF (800×600, single frame, ~82 KiB); the decoder only
/// materialises the first frame.
static GIF_800X600: &[u8] = include_bytes!("../../fixtures/gif-800x600.gif");
/// Real libwebp lossless photo (64×48, predictor/colour transforms) generated
/// with Pillow; committed in `crates/image` and pixel-verified against PIL.
static WEBP_PHOTO: &[u8] =
    include_bytes!("../../../image/tests/fixtures/webp/photo_transforms.webp");
/// Real libwebp lossless alpha image (40×32, straight alpha).
static WEBP_ALPHA: &[u8] = include_bytes!("../../../image/tests/fixtures/webp/alpha_real.webp");
/// Real libwebp lossless palette image (32×24, colour indexing).
static WEBP_PALETTE: &[u8] = include_bytes!("../../../image/tests/fixtures/webp/palette_real.webp");
/// Real libwebp lossy VP8 keyframe (1024×768, PIL q75, 40 552 B) committed in
/// `crates/image`; decoded by the in-tree VP8 decoder (from-scratch port).
static WEBP_LOSSY_1024: &[u8] =
    include_bytes!("../../../image/tests/fixtures/webp/photo_1024x768_lossy.webp");
/// Real libwebp lossy VP8 keyframe (64×48, PIL q75).
static WEBP_LOSSY_Q75: &[u8] =
    include_bytes!("../../../image/tests/fixtures/webp/photo_lossy_q75.webp");
/// Real libwebp lossy VP8 keyframe (64×48, PIL q90).
static WEBP_LOSSY_Q90: &[u8] =
    include_bytes!("../../../image/tests/fixtures/webp/photo_lossy_q90.webp");
/// Registers every benchmark into `suite`, applying `opts.scale` to the sizes.
pub fn register(suite: &mut Suite, opts: &Options) {
    register_html(suite, opts.scale);
    register_css(suite, opts.scale);
    register_layout_and_paint(suite, opts.scale);
    register_feature_e2e(suite, opts.scale);
    register_layer_effects(suite);
    register_fixture_effect_rows(suite);
    register_effect_fixture_pipelines(suite);
    register_backdrop_stage_rows(suite);
    register_png_region_encoders(suite);
    register_backdrop_blur_rows(suite);
    register_image(suite);
    register_render_pipeline(suite);
    register_png_rows(suite);
    #[cfg(feature = "fonts")]
    register_fonts(suite, opts.scale);
}

/// Counts element nodes in `document` (top-down pre-order).
fn count_elements(document: &Document) -> usize {
    collect_elements(document).len()
}

/// Collects every element [`NodeId`] in `document`.
fn collect_elements(document: &Document) -> Vec<NodeId> {
    let mut elements = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut child = document.node(document.root()).first_child;
    while let Some(node_id) = child {
        stack.push(node_id);
        child = document.node(node_id).next_sibling;
    }
    while let Some(node_id) = stack.pop() {
        if document.node(node_id).as_element().is_some() {
            elements.push(node_id);
        }
        let mut child = document.node(node_id).first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = document.node(child_id).next_sibling;
        }
    }
    elements
}
