//! Deterministic antialiased rasterizer for the parsed SVG model.
//!
//! Fills use a scanline algorithm with **4 vertical sub-rows per pixel row** and
//! analytic horizontal coverage per span (16×16-equivalent vertical resolution +
//! exact horizontal overlap), which gives smooth antialiased edges without
//! random sampling. Strokes are decomposed into segment quads, cap discs and join
//! polygons, each stamped with 4×4 sub-pixel supersampling and unioned with a
//! per-pixel `max`, then composited over the transparent RGBA canvas with
//! straight (non-premultiplied) alpha.
//!
//! Everything is deterministic: the same input bytes always produce the same
//! pixel buffer byte for byte on every platform (IEEE `f32` arithmetic only, no
//! threading and no randomness).

use std::cell::Cell;

use crate::color::Color;
use crate::document::{
    BlendMode, ClipPathSpec, ColorMatrixKind, CompositeOperator, Document, EffectRef, FillRule,
    FilterInput, FilterPrimitive, FilterSpec, LineCap, LineJoin, MAX_BLUR_RADIUS, MaskSpec, Paint,
};
use crate::gradient::GradientSpec;
use crate::path::SubPath;
use crate::pattern::Units;
use crate::pattern::{MAX_PATTERN_DEPTH, MAX_PATTERN_TILES, MIN_PATTERN_TILE, PatternSpec};
use crate::transform::Transform;
use crate::{Limits, SvgError, SvgImage};

/// Vertical sub-rows per output pixel row for fill coverage.
const SUB_ROWS: usize = 4;
/// Sub-samples per axis for stroke primitives.
const STAMP_SAMPLES: u32 = 4;
/// Total tile pixels allowed across all pattern paints in one render (#439).
/// Each tile owns an offscreen `RGBA8` canvas, so this bounds the allocation
/// work that nested patterns can trigger.
const MAX_PATTERN_TILE_PIXELS: u64 = 16 * 1024 * 1024;

/// Total glyphs rasterized across all `<text>` elements in one SVG render (#432).
/// Per-element caps (1000 texts × 10k chars) still allow 10M font lookups + rasterizations.
const MAX_SVG_GLYPHS: u64 = 50_000;

/// Upper bound on the scratch buffers a single filter chain may allocate. Each primitive
/// clones full-layer buffers (input + previous result), so a 16-primitive chain on a
/// 64M-pixel layer would otherwise copy tens of GiB (#430).
const MAX_FILTER_SCRATCH_BYTES: u64 = 512 * 1024 * 1024;

mod canvas;
mod context;
mod effects;
mod filter;
mod paint;
mod shape;
mod stroke;
mod text;

#[cfg(test)]
mod filter_scratch_tests;
#[cfg(test)]
mod tests;

// Re-export every submodule item at this level so sibling modules keep the same
// `super::*` access to shared helpers as before the split.
pub(crate) use canvas::*;
pub(crate) use context::*;
pub(crate) use effects::*;
pub(crate) use filter::*;
pub(crate) use paint::*;
pub(crate) use shape::*;
pub(crate) use stroke::*;
pub(crate) use text::*;

/// Rasterizes a document into a straight-RGBA image.
pub(crate) fn rasterize(
    document: &Document,
    width: u32,
    height: u32,
    limits: &Limits,
) -> Result<SvgImage, SvgError> {
    let (width_usize, height_usize) = (width as usize, height as usize);
    let pixel_count = width_usize
        .checked_mul(height_usize)
        .ok_or_else(|| SvgError::Limit("image size overflow".to_owned()))?;
    if pixel_count as u64 > limits.max_pixels {
        return Err(SvgError::Limit(
            "image is too large to rasterize".to_owned(),
        ));
    }
    let mut canvas = Canvas {
        width: width_usize,
        height: height_usize,
        pixels: vec![0u8; pixel_count * 4],
        coverage: vec![0.0f32; pixel_count],
    };
    let viewport = document.viewport_transform(width, height);
    if !viewport.is_finite() {
        return Err(SvgError::Invalid(
            "non-finite viewport transform".to_owned(),
        ));
    }
    let budget = Cell::new(PatternBudget::default());
    for shape in &document.shapes {
        let context = PaintContext {
            gradients: &document.gradients,
            patterns: &document.patterns,
            shape_transform: shape.transform,
            bbox: shape.bbox,
            viewport,
            depth: 0,
            budget: &budget,
        };
        if shape.clip.is_empty() && shape.mask.is_empty() && shape.filter.is_none() {
            draw_shape(&mut canvas, shape, &context)?;
        } else {
            draw_shape_with_effects(&mut canvas, shape, &context, document)?;
        }
    }
    for text in &document.texts {
        draw_text(
            &mut canvas,
            text,
            &document.gradients,
            &document.patterns,
            viewport,
            &budget,
        )?;
    }
    Ok(SvgImage {
        width,
        height,
        data: canvas.pixels,
    })
}

/// Source-over compositing of one straight-RGBA colour with source alpha `src_a`.
fn composite_pixel(pixels: &mut [u8], offset: usize, color: Color, src_a: f32) {
    let dst_a = f32::from(pixels[offset + 3]) / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        return;
    }
    let blend = |src: f32, dst: u8| -> u8 {
        let dst = f32::from(dst);
        (((src * src_a) + (dst * dst_a * (1.0 - src_a))) / out_a)
            .clamp(0.0, 255.0)
            .round() as u8
    };
    pixels[offset] = blend(f32::from(color.r), pixels[offset]);
    pixels[offset + 1] = blend(f32::from(color.g), pixels[offset + 1]);
    pixels[offset + 2] = blend(f32::from(color.b), pixels[offset + 2]);
    pixels[offset + 3] = (out_a * 255.0).clamp(0.0, 255.0).round() as u8;
}
