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

use crate::color::Color;
use crate::document::{
    BlendMode, ClipPathSpec, ColorMatrixKind, CompositeOperator, Document, EffectRef, FillRule,
    FilterInput, FilterPrimitive, FilterSpec, LineCap, LineJoin, MAX_BLUR_RADIUS, MaskSpec, Paint,
};
use crate::gradient::GradientSpec;
use crate::path::SubPath;
use crate::pattern::Units;
use crate::pattern::{MAX_PATTERN_DEPTH, MAX_PATTERN_TILES, MIN_PATTERN_TILE, PatternSpec};
use crate::text::{self, TextAnchor, TextSpec};
use crate::transform::Transform;
use crate::{Limits, SvgError, SvgImage};

/// Vertical sub-rows per output pixel row for fill coverage.
const SUB_ROWS: usize = 4;
/// Sub-samples per axis for stroke primitives.
const STAMP_SAMPLES: u32 = 4;

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
    for shape in &document.shapes {
        let context = PaintContext {
            gradients: &document.gradients,
            patterns: &document.patterns,
            shape_transform: shape.transform,
            bbox: shape.bbox,
            viewport,
            depth: 0,
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
        )?;
    }
    Ok(SvgImage {
        width,
        height,
        data: canvas.pixels,
    })
}

/// Paint resolution context (shape transform/bbox for `objectBoundingBox` units plus
/// pattern recursion depth).
struct PaintContext<'a> {
    gradients: &'a [GradientSpec],
    patterns: &'a [PatternSpec],
    shape_transform: Transform,
    bbox: (f32, f32, f32, f32),
    viewport: Transform,
    depth: usize,
}

/// Draws one shape (fill + stroke) using the coverage canvas.
fn draw_shape(
    canvas: &mut Canvas,
    shape: &crate::document::Shape,
    context: &PaintContext<'_>,
) -> Result<(), SvgError> {
    if shape.opacity <= 0.0 {
        return Ok(());
    }
    let viewport = context.viewport;
    let viewport_scale = (viewport.a * viewport.d - viewport.b * viewport.c)
        .abs()
        .sqrt();
    let device_points: Vec<SubPath> = shape
        .subpaths
        .iter()
        .map(|subpath| SubPath {
            points: subpath
                .points
                .iter()
                .map(|point| viewport.apply(point.0, point.1))
                .filter(|point| point.0.is_finite() && point.1.is_finite())
                .collect(),
            closed: subpath.closed,
        })
        .filter(|subpath| subpath.points.len() >= 2)
        .collect();
    if device_points.is_empty() {
        return Ok(());
    }
    if let Some(filled) = prepare_paint(
        shape.fill,
        context.gradients,
        context.patterns,
        shape.transform,
        shape.bbox,
        viewport,
    ) && shape.fill_opacity > 0.0
    {
        canvas.clear_coverage();
        fill_scanline(
            &device_points,
            shape.fill_rule,
            canvas.width,
            canvas.height,
            &mut canvas.coverage,
        );
        canvas.composite_paint(&filled, shape.fill_opacity * shape.opacity, context)?;
    }
    let width = shape.stroke_width * viewport_scale;
    if width > 0.0
        && width.is_finite()
        && shape.stroke_opacity > 0.0
        && let Some(stroked) = prepare_paint(
            shape.stroke,
            context.gradients,
            context.patterns,
            shape.transform,
            shape.bbox,
            viewport,
        )
    {
        canvas.clear_coverage();
        stamp_stroke(
            &device_points,
            StrokeStyle {
                width,
                cap: shape.linecap,
                join: shape.linejoin,
                miter_limit: shape.miter_limit,
            },
            canvas.width,
            canvas.height,
            &mut canvas.coverage,
        );
        canvas.composite_paint(&stroked, shape.stroke_opacity * shape.opacity, context)?;
    }
    Ok(())
}

/// Draws a shape into an offscreen layer, applies inherited clips/masks/filters and
/// composites the result over the destination canvas.
fn draw_shape_with_effects(
    canvas: &mut Canvas,
    shape: &crate::document::Shape,
    context: &PaintContext<'_>,
    document: &Document,
) -> Result<(), SvgError> {
    let mut layer = Canvas::new(canvas.width, canvas.height);
    draw_shape(&mut layer, shape, context)?;
    for clip in &shape.clip {
        apply_clip(&mut layer, document, clip, context.viewport)?;
    }
    for mask in &shape.mask {
        apply_mask(&mut layer, document, mask, context)?;
    }
    if let Some(filter) = &shape.filter
        && let Some(spec) = document.filters.get(filter.index as usize)
    {
        apply_filter(&mut layer, spec, filter, context.viewport)?;
    }
    canvas.composite_layer(&layer);
    Ok(())
}

/// Maps a reference definition's local geometry to device pixels.
fn effect_point(effect: &EffectRef, viewport: Transform, x: f32, y: f32) -> (f32, f32) {
    let (x, y) = if effect.units == Units::ObjectBoundingBox {
        (
            effect.bbox.0 + x * effect.bbox.2,
            effect.bbox.1 + y * effect.bbox.3,
        )
    } else {
        (x, y)
    };
    let (x, y) = effect.transform.apply(x, y);
    viewport.apply(x, y)
}

/// Multiplies the layer alpha by the clip coverage of a `<clipPath>` (intersection of all
/// its child shapes, each with its own fill rule).
fn apply_clip(
    layer: &mut Canvas,
    document: &Document,
    effect: &EffectRef,
    viewport: Transform,
) -> Result<(), SvgError> {
    let Some(spec): Option<&ClipPathSpec> = document.clip_paths.get(effect.index as usize) else {
        return Ok(());
    };
    let mut coverage = vec![0.0f32; layer.width * layer.height];
    for shape in &spec.shapes {
        let mut device = Vec::with_capacity(shape.subpaths.len());
        for subpath in &shape.subpaths {
            let points: Vec<(f32, f32)> = subpath
                .points
                .iter()
                .map(|(x, y)| effect_point(effect, viewport, *x, *y))
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect();
            if points.len() >= 2 {
                device.push(SubPath {
                    points,
                    closed: subpath.closed,
                });
            }
        }
        fill_scanline(
            &device,
            shape.fill_rule,
            layer.width,
            layer.height,
            &mut coverage,
        );
    }
    for (index, alpha) in layer.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let coverage = coverage[index].clamp(0.0, 1.0);
        let value = f32::from(alpha[3]) * coverage;
        alpha[3] = value.clamp(0.0, 255.0).round() as u8;
    }
    Ok(())
}

/// Multiplies the layer alpha by the luminance of a `<mask>` content rendering.
fn apply_mask(
    layer: &mut Canvas,
    document: &Document,
    effect: &EffectRef,
    context: &PaintContext<'_>,
) -> Result<(), SvgError> {
    let Some(spec): Option<&MaskSpec> = document.masks.get(effect.index as usize) else {
        return Ok(());
    };
    let viewport = context.viewport;
    let bbox = effect.bbox;
    let content_transform = match spec.content_units {
        Units::UserSpaceOnUse => Transform::IDENTITY,
        Units::ObjectBoundingBox => {
            let sx = if bbox.2.abs() > f32::EPSILON {
                bbox.2
            } else {
                1.0
            };
            let sy = if bbox.3.abs() > f32::EPSILON {
                bbox.3
            } else {
                1.0
            };
            Transform::translate(bbox.0, bbox.1).then(Transform::scale(sx, sy))
        }
    };
    let mask_viewport = viewport.then(effect.transform).then(content_transform);
    let mut content = Canvas::new(layer.width, layer.height);
    for shape in &spec.shapes {
        let shape_context = PaintContext {
            gradients: context.gradients,
            patterns: context.patterns,
            shape_transform: shape.transform,
            bbox: shape.bbox,
            viewport: mask_viewport,
            depth: context.depth,
        };
        draw_shape(&mut content, shape, &shape_context)?;
    }
    // Reference region in the element's user space, then device pixels.
    let region = mask_region_pixels(
        layer,
        effect,
        viewport,
        &spec.x,
        &spec.y,
        &spec.width,
        &spec.height,
    )?;
    for (index, alpha) in layer.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (index % layer.width) as f32 + 0.5;
        let y = (index / layer.width) as f32 + 0.5;
        let inside = region.is_some_and(|(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
        if !inside {
            alpha[3] = 0;
            continue;
        }
        let offset = index * 4;
        let mask_alpha = f32::from(content.pixels[offset + 3]) / 255.0;
        let luminance = (0.2126 * f32::from(content.pixels[offset])
            + 0.7152 * f32::from(content.pixels[offset + 1])
            + 0.0722 * f32::from(content.pixels[offset + 2]))
            / 255.0;
        let coverage = (mask_alpha * luminance).clamp(0.0, 1.0);
        alpha[3] = (f32::from(alpha[3]) * coverage).clamp(0.0, 255.0).round() as u8;
    }
    Ok(())
}

/// Computes the device-space rectangle of a mask/filter region.
fn mask_region_pixels(
    layer: &Canvas,
    effect: &EffectRef,
    viewport: Transform,
    x: &crate::document::RegionLength,
    y: &crate::document::RegionLength,
    width: &crate::document::RegionLength,
    height: &crate::document::RegionLength,
) -> Result<Option<(f32, f32, f32, f32)>, SvgError> {
    let bbox = effect.bbox;
    let (x, y, w, h) = if effect.units == Units::ObjectBoundingBox {
        // In objectBoundingBox units plain numbers are fractions of the reference box.
        let fraction = |value: &crate::document::RegionLength, base: f32| -> f32 {
            match value {
                crate::document::RegionLength::Number(number) => number * base,
                crate::document::RegionLength::Percent(percent) => percent * base,
            }
        };
        (
            bbox.0 + fraction(x, bbox.2),
            bbox.1 + fraction(y, bbox.3),
            fraction(width, bbox.2),
            fraction(height, bbox.3),
        )
    } else {
        (
            x.resolve(bbox.2),
            y.resolve(bbox.3),
            width.resolve(bbox.2),
            height.resolve(bbox.3),
        )
    };
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0
    {
        return Ok(None);
    }
    let map = |px: f32, py: f32| {
        let (rx, ry) = effect.transform.apply(px, py);
        viewport.apply(rx, ry)
    };
    let corners = [map(x, y), map(x + w, y), map(x, y + h), map(x + w, y + h)];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (px, py) in corners {
        if px.is_finite() && py.is_finite() {
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return Ok(None);
    }
    let _ = layer;
    Ok(Some((
        min_x.clamp(0.0, layer.width as f32),
        min_y.clamp(0.0, layer.height as f32),
        max_x.clamp(0.0, layer.width as f32),
        max_y.clamp(0.0, layer.height as f32),
    )))
}

/// Applies a supported filter chain to the layer pixels.
///
/// Primitives are resolved in order with `in`/`in2`/`result` references
/// (`SourceGraphic`, `SourceAlpha`, `Previous` or a named result); the last primitive's
/// output is the filter result. The filter region clips the final result.
fn apply_filter(
    layer: &mut Canvas,
    spec: &FilterSpec,
    effect: &EffectRef,
    viewport: Transform,
) -> Result<(), SvgError> {
    if spec.primitives.is_empty() {
        return Ok(());
    }
    let region = mask_region_pixels(
        layer,
        effect,
        viewport,
        &spec.x,
        &spec.y,
        &spec.width,
        &spec.height,
    )?;
    let transform = viewport.then(effect.transform);
    let scale = (transform.a * transform.d - transform.b * transform.c)
        .abs()
        .sqrt();
    if !scale.is_finite() || scale <= 0.0 {
        return Ok(());
    }
    let source = layer.pixels.clone();
    let mut source_alpha = source.clone();
    for pixel in source_alpha.as_chunks_mut::<4>().0 {
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
    }
    let mut results: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    let mut previous = source.clone();
    // Filter Effects: a reference to a result that does not exist (typo, forward reference)
    // resolves to transparent black, not to the source graphic.
    let transparent = vec![0u8; source.len()];
    for primitive in &spec.primitives {
        let input = |reference: &FilterInput| -> Vec<u8> {
            match reference {
                FilterInput::SourceGraphic => source.clone(),
                FilterInput::SourceAlpha => source_alpha.clone(),
                FilterInput::Previous => previous.clone(),
                FilterInput::Named(name) => results
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| transparent.clone()),
            }
        };
        let (pixels, result) = match primitive {
            FilterPrimitive::GaussianBlur {
                std_deviation,
                input: reference,
                result,
            } => {
                let mut pixels = input(reference);
                box_blur(
                    &mut pixels,
                    layer.width,
                    layer.height,
                    (*std_deviation * scale).max(0.0),
                );
                (pixels, result)
            }
            FilterPrimitive::Offset {
                dx,
                dy,
                input: reference,
                result,
            } => {
                let mut pixels = input(reference);
                offset_pixels(
                    &mut pixels,
                    layer.width,
                    layer.height,
                    (dx * scale).round() as i32,
                    (dy * scale).round() as i32,
                );
                (pixels, result)
            }
            FilterPrimitive::Flood {
                color,
                opacity,
                result,
            } => {
                let alpha = (f32::from(color.a) / 255.0 * opacity).clamp(0.0, 1.0);
                let mut pixels = vec![0u8; layer.pixels.len()];
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel[0] = color.r;
                    pixel[1] = color.g;
                    pixel[2] = color.b;
                    pixel[3] = (alpha * 255.0).round() as u8;
                }
                (pixels, result)
            }
            FilterPrimitive::ColorMatrix {
                kind,
                input: reference,
                result,
            } => (apply_color_matrix(&input(reference), kind), result),
            FilterPrimitive::Blend {
                mode,
                input: reference,
                input2,
                result,
            } => {
                let src = input(reference);
                let dst = input(input2);
                (blend_images(&src, &dst, *mode), result)
            }
            FilterPrimitive::Merge { inputs, result } => {
                let mut pixels = vec![0u8; layer.pixels.len()];
                for reference in inputs {
                    pixels = composite_over(&input(reference), &pixels);
                }
                (pixels, result)
            }
            FilterPrimitive::Composite {
                operator,
                input: reference,
                input2,
                result,
            } => {
                let src = input(reference);
                let dst = input(input2);
                (composite_images_ext(&src, &dst, operator), result)
            }
        };
        previous = pixels.clone();
        if let Some(name) = result {
            results.insert(name.clone(), pixels.clone());
        }
    }
    let mut output = previous;
    if let Some((x0, y0, x1, y1)) = region {
        for y in 0..layer.height {
            for x in 0..layer.width {
                let inside = (x as f32 + 0.5) >= x0
                    && (x as f32 + 0.5) < x1
                    && (y as f32 + 0.5) >= y0
                    && (y as f32 + 0.5) < y1;
                if !inside {
                    let offset = (y * layer.width + x) * 4;
                    output[offset + 3] = 0;
                }
            }
        }
    }
    layer.pixels = output;
    Ok(())
}

fn premultiply(pixel: &[u8]) -> [f32; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    [
        f32::from(pixel[0]) / 255.0 * alpha,
        f32::from(pixel[1]) / 255.0 * alpha,
        f32::from(pixel[2]) / 255.0 * alpha,
        alpha,
    ]
}

fn unpremultiply(premultiplied: [f32; 4]) -> [u8; 4] {
    let alpha = premultiplied[3].clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        ((premultiplied[0].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        ((premultiplied[1].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        ((premultiplied[2].clamp(0.0, 1.0) / alpha) * 255.0).round() as u8,
        (alpha * 255.0).round() as u8,
    ]
}

fn composite_over(src: &[u8], dst: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for (index, target) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = s[3] + d[3] * (1.0 - s[3]);
        let mut result = [0.0f32; 4];
        for channel in 0..3 {
            result[channel] = s[channel] + d[channel] * (1.0 - s[3]);
        }
        result[3] = alpha;
        target.copy_from_slice(&unpremultiply(result));
    }
    out
}

fn composite_images_ext(src: &[u8], dst: &[u8], operator: &CompositeOperator) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for index in 0..src.len() / 4 {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = match operator {
            CompositeOperator::Over => s[3] + d[3] * (1.0 - s[3]),
            CompositeOperator::In => s[3] * d[3],
            CompositeOperator::Out => s[3] * (1.0 - d[3]),
            CompositeOperator::Atop => d[3],
            CompositeOperator::Arithmetic(k) => {
                k[0] * s[3] * d[3] + k[1] * s[3] + k[2] * d[3] + k[3]
            }
        }
        .clamp(0.0, 1.0);
        let mut result = [0.0f32; 4];
        match operator {
            CompositeOperator::Over => {
                for channel in 0..3 {
                    result[channel] = s[channel] + d[channel] * (1.0 - s[3]);
                }
            }
            CompositeOperator::In => {
                for channel in 0..3 {
                    result[channel] = s[channel] * d[3];
                }
            }
            CompositeOperator::Out => {
                for channel in 0..3 {
                    result[channel] = s[channel] * (1.0 - d[3]);
                }
            }
            CompositeOperator::Atop => {
                for channel in 0..3 {
                    result[channel] = s[channel] * d[3] + d[channel] * (1.0 - s[3]);
                }
            }
            CompositeOperator::Arithmetic(k) => {
                for channel in 0..3 {
                    result[channel] = k[0] * s[channel] * d[channel]
                        + k[1] * s[channel]
                        + k[2] * d[channel]
                        + k[3];
                }
            }
        }
        result[3] = alpha;
        let offset = index * 4;
        out[offset..offset + 4].copy_from_slice(&unpremultiply(result));
    }
    out
}

fn blend_images(src: &[u8], dst: &[u8], mode: BlendMode) -> Vec<u8> {
    let mut out = vec![0u8; src.len()];
    for index in 0..src.len() / 4 {
        let s = premultiply(&src[index * 4..index * 4 + 4]);
        let d = premultiply(&dst[index * 4..index * 4 + 4]);
        let alpha = s[3] + d[3] * (1.0 - s[3]);
        let mut result = [0.0f32; 4];
        for channel in 0..3 {
            let src_channel = if s[3] > 0.0 { s[channel] / s[3] } else { 0.0 };
            let dst_channel = if d[3] > 0.0 { d[channel] / d[3] } else { 0.0 };
            let blended = match mode {
                BlendMode::Normal => src_channel,
                BlendMode::Multiply => src_channel * dst_channel,
                BlendMode::Screen => src_channel + dst_channel - src_channel * dst_channel,
                BlendMode::Darken => src_channel.min(dst_channel),
                BlendMode::Lighten => src_channel.max(dst_channel),
            };
            // CSS compositing formula for feBlend.
            result[channel] =
                (1.0 - d[3]) * s[channel] + (1.0 - s[3]) * d[channel] + s[3] * d[3] * blended;
        }
        result[3] = alpha;
        let offset = index * 4;
        out[offset..offset + 4].copy_from_slice(&unpremultiply(result));
    }
    out
}

fn apply_color_matrix(pixels: &[u8], kind: &ColorMatrixKind) -> Vec<u8> {
    let saturate_matrix = |s: f32| {
        [
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    };
    let hue_matrix = |angle_deg: f32| {
        let angle = angle_deg.to_radians();
        let (cos, sin) = (angle.cos(), angle.sin());
        [
            0.213 + cos * 0.787 - sin * 0.213,
            0.715 - cos * 0.715 - sin * 0.715,
            0.072 - cos * 0.072 + sin * 0.928,
            0.0,
            0.0,
            0.213 - cos * 0.213 + sin * 0.143,
            0.715 + cos * 0.285 + sin * 0.140,
            0.072 - cos * 0.072 - sin * 0.283,
            0.0,
            0.0,
            0.213 - cos * 0.213 - sin * 0.787,
            0.715 - cos * 0.715 + sin * 0.715,
            0.072 + cos * 0.928 + sin * 0.072,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    };
    let matrix: Vec<f32> = match kind {
        ColorMatrixKind::Matrix(values) => values.to_vec(),
        ColorMatrixKind::Saturate(value) => saturate_matrix(*value).to_vec(),
        ColorMatrixKind::HueRotate(angle) => hue_matrix(*angle).to_vec(),
        ColorMatrixKind::LuminanceToAlpha => vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2126,
            0.7152, 0.0722, 0.0,
        ],
    };
    let mut out = vec![0u8; pixels.len()];
    for index in 0..pixels.len() / 4 {
        let r = f32::from(pixels[index * 4]) / 255.0;
        let g = f32::from(pixels[index * 4 + 1]) / 255.0;
        let b = f32::from(pixels[index * 4 + 2]) / 255.0;
        let a = f32::from(pixels[index * 4 + 3]) / 255.0;
        let mut channels = [r, g, b, a, 1.0];
        for (row, item) in matrix.as_chunks::<5>().0.iter().enumerate().take(4) {
            if row == 3 && !matches!(kind, ColorMatrixKind::LuminanceToAlpha) {
                // Generic matrices keep the alpha row; luminanceToAlpha sets it below.
            }
            channels[row] =
                (item[0] * r + item[1] * g + item[2] * b + item[3] * a + item[4]).clamp(0.0, 1.0);
        }
        if matches!(kind, ColorMatrixKind::LuminanceToAlpha) {
            let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            channels = [0.0, 0.0, 0.0, luminance, 1.0];
        }
        let offset = index * 4;
        out[offset] = (channels[0] * 255.0).round() as u8;
        out[offset + 1] = (channels[1] * 255.0).round() as u8;
        out[offset + 2] = (channels[2] * 255.0).round() as u8;
        out[offset + 3] = (channels[3] * 255.0).round() as u8;
    }
    out
}

/// Deterministic separable box blur (3 passes ≈ Gaussian) on straight RGBA pixels.
fn box_blur(pixels: &mut [u8], width: usize, height: usize, sigma: f32) {
    if width == 0 || height == 0 || !sigma.is_finite() || sigma <= 0.0 {
        return;
    }
    // Three separable box passes (H+V each) approximate a Gaussian; pick the radius whose
    // accumulated variance (6 * (2r+1)^2-1 / 12) matches sigma².
    let radius = (((1.0 + 2.0 * sigma * sigma).sqrt() - 1.0) * 0.5).round() as u32;
    let radius = radius.clamp(1, MAX_BLUR_RADIUS);
    let area = width.saturating_mul(height);
    // Anti-DoS: cap the per-pass sample budget by shrinking the radius for large layers.
    let budget = 64_000_000u64;
    let per_pass = area as u64 * u64::from(radius) * 2;
    let radius = if per_pass == 0 {
        return;
    } else if per_pass > budget {
        (budget / (area as u64 * 2)).max(1) as u32
    } else {
        radius
    };
    for _ in 0..3 {
        blur_pass(pixels, width, height, radius, true);
        blur_pass(pixels, width, height, radius, false);
    }
}

fn blur_pass(pixels: &mut [u8], width: usize, height: usize, radius: u32, horizontal: bool) {
    if radius == 0 {
        return;
    }
    let mut temp = pixels.to_vec();
    let length = if horizontal { width } else { height };
    let lines = if horizontal { height } else { width };
    let radius = radius as isize;
    let window = (2 * radius + 1) as u32;
    let clamp = |index: isize| -> usize { index.clamp(0, length as isize - 1) as usize };
    for line in 0..lines {
        let mut accum = [0u32; 4];
        // Seed the window for the first pixel (edge-clamped).
        for offset in -radius..=radius {
            let index = clamp(offset);
            let (x, y) = if horizontal {
                (index, line)
            } else {
                (line, index)
            };
            let pixel = (y * width + x) * 4;
            for channel in 0..4 {
                accum[channel] += u32::from(pixels[pixel + channel]);
            }
        }
        for offset in 0..length {
            let (x, y) = if horizontal {
                (offset, line)
            } else {
                (line, offset)
            };
            let pixel = (y * width + x) * 4;
            for channel in 0..4 {
                temp[pixel + channel] = (accum[channel] / window) as u8;
            }
            let remove = clamp(offset as isize - radius);
            let add = clamp(offset as isize + radius + 1);
            let (rx, ry) = if horizontal {
                (remove, line)
            } else {
                (line, remove)
            };
            let (ax, ay) = if horizontal { (add, line) } else { (line, add) };
            let remove_pixel = (ry * width + rx) * 4;
            let add_pixel = (ay * width + ax) * 4;
            for channel in 0..4 {
                accum[channel] = accum[channel]
                    .saturating_sub(u32::from(pixels[remove_pixel + channel]))
                    .saturating_add(u32::from(pixels[add_pixel + channel]));
            }
        }
    }
    pixels.copy_from_slice(&temp);
}

fn offset_pixels(pixels: &mut [u8], width: usize, height: usize, dx: i32, dy: i32) {
    let source = pixels.to_vec();
    pixels.fill(0);
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                continue;
            }
            let source_index = (sy as usize * width + sx as usize) * 4;
            let target_index = (y as usize * width + x as usize) * 4;
            pixels[target_index..target_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }
}

/// Draws a `<text>` element: per-glyph advance positioning (no shaping/kerning),
/// `text-anchor`, `letter-spacing`, font size/weight/style and fill/stroke.
type TextPathSegment = (f32, (f32, f32), (f32, f32));

fn segment_at(segments: &[TextPathSegment], distance: f32) -> Option<TextPathSegment> {
    for &(start, from, to) in segments {
        let length = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        if distance >= start && distance <= start + length {
            return Some((start, from, to));
        }
    }
    None
}

fn draw_text(
    canvas: &mut Canvas,
    spec: &TextSpec,
    gradients: &[GradientSpec],
    patterns: &[PatternSpec],
    viewport: Transform,
) -> Result<(), SvgError> {
    if spec.opacity <= 0.0 || spec.text.is_empty() {
        return Ok(());
    }
    let Some(database) = text::bundled_database() else {
        return Ok(());
    };
    let matrix = viewport.then(spec.transform);
    let scale = (matrix.a * matrix.d - matrix.b * matrix.c).abs().sqrt();
    if !scale.is_finite() || scale <= 0.0 {
        return Ok(());
    }
    let size = spec.font_size * scale;
    if !size.is_finite() || size <= 0.0 || size > 4096.0 {
        return Ok(());
    }
    let families = text::families(&spec.font_family);
    let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
    let style = if spec.italic {
        fonts::FontStyle::Italic
    } else {
        fonts::FontStyle::Normal
    };
    let spacing = spec.letter_spacing * scale;
    let char_count = spec.text.chars().count().max(1) as f32;
    let measured = database
        .measure_text(&family_refs, spec.font_weight, style, size, &spec.text)
        .unwrap_or(0.0);
    let total = measured + spacing * (char_count - 1.0).max(0.0);
    let anchor_shift = match spec.anchor {
        TextAnchor::Start => 0.0,
        TextAnchor::Middle => total * 0.5,
        TextAnchor::End => total,
    };
    canvas.clear_coverage();
    if let Some(text_path) = &spec.text_path {
        // Glyph advance along the flattened path with auto-rotation (tangent angle, flipped
        // when the text would be upside down), optional textLength adjustment and
        // side="right" baseline offset (approximate normal offset, documented subset).
        let mut lengths: Vec<TextPathSegment> = Vec::new();
        let mut total_length = 0.0f32;
        for subpath in &text_path.subpaths {
            for window in subpath.points.windows(2) {
                let (x0, y0) = window[0];
                let (x1, y1) = window[1];
                let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                if length.is_finite() && length > 0.0 {
                    lengths.push((total_length, (x0, y0), (x1, y1)));
                    total_length += length;
                }
            }
        }
        if total_length > 0.0 {
            let offset = if text_path.percent {
                text_path.start_offset * total_length
            } else {
                text_path.start_offset
            };
            let mut natural = 0.0f32;
            for character in spec.text.chars() {
                if let Ok(mask) =
                    database.rasterize_char(&family_refs, spec.font_weight, style, size, character)
                {
                    natural += mask.advance + spacing;
                }
            }
            let length_scale = spec
                .text_length
                .filter(|target| *target > 0.0 && natural > 0.0)
                .map(|target| target / natural)
                .unwrap_or(1.0);
            let character_count = spec.text.chars().count().max(1) as f32;
            let spacing_extra = if spec.spacing_and_glyphs || spec.text_length.is_none() {
                0.0
            } else {
                (natural * (length_scale - 1.0)) / (character_count - 1.0).max(1.0)
            };
            let glyph_size = if spec.spacing_and_glyphs {
                size * length_scale
            } else {
                size
            };
            let mut distance = (offset - anchor_shift).clamp(0.0, total_length);
            for character in spec.text.chars() {
                let Some((start, from, to)) = segment_at(&lengths, distance) else {
                    break;
                };
                let length = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
                let t = ((distance - start) / length).clamp(0.0, 1.0);
                let point = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                let (px, py) = matrix.apply(point.0, point.1);
                let tangent = matrix.apply(to.0, to.1);
                let mut angle = (tangent.1 - py).atan2(tangent.0 - px);
                // Keep glyphs upright: flip on the left half-plane / descending segments.
                if angle > std::f32::consts::FRAC_PI_2 || angle < -std::f32::consts::FRAC_PI_2 {
                    angle += std::f32::consts::PI;
                }
                let normal = if text_path.side_right {
                    (angle.sin(), -angle.cos())
                } else {
                    (0.0, 0.0)
                };
                let side_offset = if text_path.side_right {
                    size * 0.9
                } else {
                    0.0
                };
                let gx = px + normal.0 * side_offset;
                let gy = py + normal.1 * side_offset;
                let Ok(mask) = database.rasterize_char(
                    &family_refs,
                    spec.font_weight,
                    style,
                    glyph_size,
                    character,
                ) else {
                    continue;
                };
                canvas.add_mask_rotated(gx, gy, &mask, angle);
                distance += mask.advance + spacing + spacing_extra;
            }
        }
    } else {
        let (start_x, baseline_y) = matrix.apply(spec.x + spec.dx, spec.y + spec.dy);
        let mut pen_x = start_x - anchor_shift;
        for character in spec.text.chars() {
            if !pen_x.is_finite() {
                break;
            }
            let Ok(mask) =
                database.rasterize_char(&family_refs, spec.font_weight, style, size, character)
            else {
                continue;
            };
            canvas.add_mask(pen_x + mask.left, baseline_y + mask.top, &mask);
            pen_x += mask.advance + spacing;
        }
    }
    let context = PaintContext {
        gradients,
        patterns,
        shape_transform: spec.transform,
        bbox: spec.bbox,
        viewport,
        depth: 0,
    };
    if let Some(filled) = prepare_paint(
        spec.fill,
        gradients,
        patterns,
        spec.transform,
        spec.bbox,
        viewport,
    ) && spec.fill_opacity > 0.0
    {
        canvas.composite_paint(&filled, spec.fill_opacity * spec.opacity, &context)?;
    }
    if spec.stroke_opacity > 0.0
        && let Some(stroked) = prepare_paint(
            spec.stroke,
            gradients,
            patterns,
            spec.transform,
            spec.bbox,
            viewport,
        )
    {
        let radius = ((spec.stroke_width * scale) * 0.5).round().clamp(1.0, 8.0) as i32;
        canvas.dilate_coverage(radius);
        canvas.composite_paint(&stroked, spec.stroke_opacity * spec.opacity, &context)?;
    }
    Ok(())
}

/// A paint prepared for compositing: solid colour, or a gradient evaluator
/// (inverse device → gradient-parameter matrix precomputed once per shape).
enum PreparedPaint<'a> {
    Solid(Color),
    Gradient {
        spec: &'a GradientSpec,
        inverse: crate::transform::Transform,
    },
    Pattern {
        spec: &'a PatternSpec,
    },
}

/// Resolves a paint value against the document gradients for one shape.
fn prepare_paint<'a>(
    paint: Paint,
    gradients: &'a [GradientSpec],
    patterns: &'a [PatternSpec],
    shape_transform: crate::transform::Transform,
    bbox: (f32, f32, f32, f32),
    viewport: crate::transform::Transform,
) -> Option<PreparedPaint<'a>> {
    match paint {
        Paint::None => None,
        Paint::Solid(color) => (color.a > 0).then_some(PreparedPaint::Solid(color)),
        Paint::Pattern(index) => patterns
            .get(index as usize)
            .filter(|pattern| pattern.usable())
            .map(|spec| PreparedPaint::Pattern { spec }),
        Paint::Gradient(index) => {
            let spec = gradients.get(index as usize)?;
            // device = viewport * shape_transform * units_matrix * gradient_transform
            let matrix = viewport
                .then(shape_transform)
                .then(spec.parameter_to_user(bbox));
            let inverse = matrix.inverse()?;
            Some(PreparedPaint::Gradient { spec, inverse })
        }
    }
}

/// RGBA canvas with a reusable coverage buffer.
struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    coverage: Vec<f32>,
}

impl Canvas {
    fn clear_coverage(&mut self) {
        self.coverage.fill(0.0);
    }

    /// Composites the current coverage buffer with a resolved paint.
    fn composite_paint(
        &mut self,
        paint: &PreparedPaint<'_>,
        opacity: f32,
        context: &PaintContext<'_>,
    ) -> Result<(), SvgError> {
        match paint {
            PreparedPaint::Solid(color) => {
                let alpha = (f32::from(color.a) / 255.0) * opacity.clamp(0.0, 1.0);
                self.composite(*color, alpha);
                Ok(())
            }
            PreparedPaint::Gradient { spec, inverse } => {
                self.composite_gradient(spec, *inverse, opacity.clamp(0.0, 1.0));
                Ok(())
            }
            PreparedPaint::Pattern { spec } => self.composite_pattern(spec, opacity, context),
        }
    }

    /// Tiles a pattern over the current coverage (clipped by it), rendering the pattern
    /// content once per tile. Tile count is capped by [`MAX_PATTERN_TILES`].
    fn composite_pattern(
        &mut self,
        spec: &PatternSpec,
        opacity: f32,
        context: &PaintContext<'_>,
    ) -> Result<(), SvgError> {
        if context.depth >= MAX_PATTERN_DEPTH || !spec.usable() {
            return Ok(());
        }
        let (px, py, width, height) = spec.tile_box(context.bbox);
        if width.abs() < MIN_PATTERN_TILE || height.abs() < MIN_PATTERN_TILE {
            return Ok(());
        }
        let content = spec.content_transform(context.bbox);
        // Pattern space → device pixels (patternTransform applies to the whole pattern).
        let base = context
            .viewport
            .then(context.shape_transform)
            .then(spec.transform);
        let Some(inverse) = base.inverse() else {
            return Ok(());
        };
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (x, y) in [
            (0.0, 0.0),
            (self.width as f32, 0.0),
            (0.0, self.height as f32),
            (self.width as f32, self.height as f32),
        ] {
            let (px_space, py_space) = inverse.apply(x, y);
            if !px_space.is_finite() || !py_space.is_finite() {
                return Ok(());
            }
            min_x = min_x.min(px_space);
            min_y = min_y.min(py_space);
            max_x = max_x.max(px_space);
            max_y = max_y.max(py_space);
        }
        // Compute the span in f64: a huge but finite `patternTransform` can exceed i64, and
        // clamping the absolute index (or adding 1 to a saturated cast) would either panic
        // or silently move a valid pattern whose phase is beyond the tile budget. Only an
        // over-budget *span* is rejected.
        let i0f = ((min_x - px) / width).floor();
        let i1f = ((max_x - px) / width).ceil();
        let j0f = ((min_y - py) / height).floor();
        let j1f = ((max_y - py) / height).ceil();
        let columns = i1f - i0f + 3.0;
        let rows = j1f - j0f + 3.0;
        if !columns.is_finite() || !rows.is_finite() || columns * rows > MAX_PATTERN_TILES as f32 {
            return Err(SvgError::Limit(
                "pattern tile budget exceeded (too many repetitions)".to_owned(),
            ));
        }
        if columns <= 0.0 || rows <= 0.0 {
            return Ok(());
        }
        let i0 = (i0f as i64).saturating_sub(1);
        let i1 = (i1f as i64).saturating_add(1);
        let j0 = (j0f as i64).saturating_sub(1);
        let j1 = (j1f as i64).saturating_add(1);
        for column in i0..=i1 {
            for row in j0..=j1 {
                let origin =
                    Transform::translate(px + column as f32 * width, py + row as f32 * height);
                let content_to_device = base.then(origin).then(content);
                // Tile quad in device pixels for clipping/canvas sizing.
                let quad = base.then(origin);
                let mut tile_min_x = f32::INFINITY;
                let mut tile_min_y = f32::INFINITY;
                let mut tile_max_x = f32::NEG_INFINITY;
                let mut tile_max_y = f32::NEG_INFINITY;
                for (x, y) in [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)] {
                    let (dx, dy) = quad.apply(x, y);
                    if !dx.is_finite() || !dy.is_finite() {
                        continue;
                    }
                    tile_min_x = tile_min_x.min(dx);
                    tile_min_y = tile_min_y.min(dy);
                    tile_max_x = tile_max_x.max(dx);
                    tile_max_y = tile_max_y.max(dy);
                }
                if !tile_min_x.is_finite() || !tile_min_y.is_finite() {
                    continue;
                }
                let x0 = (tile_min_x.floor() as i64).max(0);
                let y0 = (tile_min_y.floor() as i64).max(0);
                let x1 = (tile_max_x.ceil() as i64).min(self.width as i64);
                let y1 = (tile_max_y.ceil() as i64).min(self.height as i64);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let tile_width = (x1 - x0) as usize;
                let tile_height = (y1 - y0) as usize;
                let mut tile = Canvas::new(tile_width, tile_height);
                let local =
                    Transform::translate(-(x0 as f32), -(y0 as f32)).then(content_to_device);
                for shape in &spec.shapes {
                    let shape_context = PaintContext {
                        gradients: context.gradients,
                        patterns: context.patterns,
                        shape_transform: shape.transform,
                        bbox: shape.bbox,
                        viewport: local,
                        depth: context.depth + 1,
                    };
                    draw_shape(&mut tile, shape, &shape_context)?;
                }
                // Composite the tile only where the main shape coverage is positive.
                for ty in 0..tile_height {
                    for tx in 0..tile_width {
                        let main_x = x0 as usize + tx;
                        let main_y = y0 as usize + ty;
                        let main_index = main_y * self.width + main_x;
                        let coverage = self.coverage[main_index].clamp(0.0, 1.0);
                        if coverage <= 0.0 {
                            continue;
                        }
                        let tile_index = (ty * tile_width + tx) * 4;
                        let alpha = f32::from(tile.pixels[tile_index + 3]) / 255.0;
                        if alpha <= 0.0 {
                            continue;
                        }
                        let color = Color {
                            r: tile.pixels[tile_index],
                            g: tile.pixels[tile_index + 1],
                            b: tile.pixels[tile_index + 2],
                            a: 255,
                        };
                        composite_pixel(
                            &mut self.pixels,
                            main_index * 4,
                            color,
                            alpha * coverage * opacity.clamp(0.0, 1.0),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn new(width: usize, height: usize) -> Self {
        let pixel_count = width.saturating_mul(height);
        Self {
            width,
            height,
            pixels: vec![0u8; pixel_count * 4],
            coverage: vec![0.0f32; pixel_count],
        }
    }

    /// Composites another canvas's straight-RGBA pixels over this one (source-over).
    fn composite_layer(&mut self, layer: &Canvas) {
        for (index, pixel) in layer.pixels.as_chunks::<4>().0.iter().enumerate() {
            let alpha = f32::from(pixel[3]) / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let color = Color {
                r: pixel[0],
                g: pixel[1],
                b: pixel[2],
                a: 255,
            };
            composite_pixel(&mut self.pixels, index * 4, color, alpha);
        }
    }

    /// Adds a glyph coverage mask at the given device position (max coverage).
    /// Adds a glyph mask rotated by `angle` radians around its baseline origin `(x, y)`.
    fn add_mask_rotated(&mut self, x: f32, y: f32, mask: &fonts::GlyphMask, angle: f32) {
        if angle.abs() < 1e-4 {
            self.add_mask(x, y, mask);
            return;
        }
        let (sin, cos) = angle.sin_cos();
        let origin_x = x.round() as i64;
        let origin_y = y.round() as i64;
        for row in 0..mask.height {
            for column in 0..mask.width {
                let value = mask.data[(row * mask.width + column) as usize];
                if value == 0 {
                    continue;
                }
                let local_x = column as f32 + mask.left;
                let local_y = row as f32 + mask.top;
                let dx = local_x * cos - local_y * sin;
                let dy = local_x * sin + local_y * cos;
                let px = origin_x + dx.round() as i64;
                let py = origin_y + dy.round() as i64;
                if px < 0 || py < 0 || px >= self.width as i64 || py >= self.height as i64 {
                    continue;
                }
                let index = py as usize * self.width + px as usize;
                let alpha = f32::from(value) / 255.0;
                if alpha > self.coverage[index] {
                    self.coverage[index] = alpha;
                }
            }
        }
    }

    fn add_mask(&mut self, x: f32, y: f32, mask: &fonts::GlyphMask) {
        let origin_x = x.round() as i64;
        let origin_y = y.round() as i64;
        for row in 0..mask.height {
            let py = origin_y + i64::from(row);
            if py < 0 || py >= self.height as i64 {
                continue;
            }
            for column in 0..mask.width {
                let px = origin_x + i64::from(column);
                if px < 0 || px >= self.width as i64 {
                    continue;
                }
                let value = mask.data[(row * mask.width + column) as usize];
                if value == 0 {
                    continue;
                }
                let index = py as usize * self.width + px as usize;
                let alpha = f32::from(value) / 255.0;
                if alpha > self.coverage[index] {
                    self.coverage[index] = alpha;
                }
            }
        }
    }

    /// Dilates the coverage buffer by `radius` pixels (approximate text stroke).
    fn dilate_coverage(&mut self, radius: i32) {
        if radius <= 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let source = self.coverage.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                if source[index] >= 1.0 {
                    continue;
                }
                let mut best = source[index];
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let sx = x as i64 + i64::from(dx);
                        let sy = y as i64 + i64::from(dy);
                        if sx < 0 || sy < 0 || sx >= self.width as i64 || sy >= self.height as i64 {
                            continue;
                        }
                        best = best.max(source[sy as usize * self.width + sx as usize]);
                    }
                }
                self.coverage[index] = best;
            }
        }
    }

    /// Composites a gradient: evaluates the gradient at each covered pixel centre
    /// (deterministic, no sampling).
    fn composite_gradient(
        &mut self,
        spec: &GradientSpec,
        inverse: crate::transform::Transform,
        opacity: f32,
    ) {
        let width = self.width;
        let pixels = &mut self.pixels;
        for (index, coverage) in self.coverage.iter().enumerate() {
            let coverage = coverage.clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let x = (index % width) as f32 + 0.5;
            let y = (index / width) as f32 + 0.5;
            let (tx, ty) = inverse.apply(x, y);
            if !tx.is_finite() || !ty.is_finite() {
                continue;
            }
            let color = spec.evaluate(spec.parameter(tx, ty));
            let src_a = coverage * opacity * (f32::from(color.a) / 255.0);
            if src_a <= 0.0 {
                continue;
            }
            composite_pixel(pixels, index * 4, color, src_a);
        }
    }

    /// Composites the current coverage buffer with `color` (source-over, straight alpha).
    fn composite(&mut self, color: Color, alpha: f32) {
        let pixels = &mut self.pixels;
        for (index, coverage) in self.coverage.iter().enumerate() {
            let src_a = coverage.clamp(0.0, 1.0) * alpha;
            if src_a <= 0.0 {
                continue;
            }
            composite_pixel(pixels, index * 4, color, src_a);
        }
    }
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

/// Scanline fill with 4 vertical sub-rows and analytic horizontal coverage.
fn fill_scanline(
    subpaths: &[SubPath],
    rule: FillRule,
    width: usize,
    height: usize,
    coverage: &mut [f32],
) {
    // Build edges once; horizontal edges never cross a scanline.
    let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
    for subpath in subpaths {
        let points = &subpath.points;
        if points.len() < 2 {
            continue;
        }
        for index in 0..points.len() {
            let start = points[index];
            let end = points[(index + 1) % points.len()];
            if (start.1 - end.1).abs() <= f32::EPSILON {
                continue;
            }
            let direction = if end.1 > start.1 { 1 } else { -1 };
            edges.push((start.0, start.1, end.0, end.1, direction));
        }
    }
    if edges.is_empty() {
        return;
    }
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for pixel_y in 0..height {
        for sub_row in 0..SUB_ROWS {
            let y = pixel_y as f32 + (sub_row as f32 + 0.5) / SUB_ROWS as f32;
            crossings.clear();
            for &(x0, y0, x1, y1, direction) in &edges {
                let inside = (y0 <= y && y < y1) || (y1 <= y && y < y0);
                if !inside {
                    continue;
                }
                let t = (y - y0) / (y1 - y0);
                let x = x0 + t * (x1 - x0);
                if x.is_finite() {
                    crossings.push((x, direction));
                }
            }
            if crossings.len() < 2 {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0i32;
            let mut parity = 0i32;
            let base = pixel_y * width;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                parity += pair[0].1.abs();
                let inside = match rule {
                    FillRule::NonZero => winding != 0,
                    FillRule::EvenOdd => parity % 2 != 0,
                };
                if !inside {
                    continue;
                }
                add_span_coverage(
                    pair[0].0,
                    pair[1].0,
                    width,
                    &mut coverage[base..base + width],
                );
            }
        }
    }
}

/// Adds horizontal span `[x0, x1)` coverage to one pixel row (`1/SUB_ROWS` weight).
fn add_span_coverage(x0: f32, x1: f32, width: usize, row: &mut [f32]) {
    if !x0.is_finite() || !x1.is_finite() || x1 <= x0 {
        return;
    }
    let weight = 1.0 / SUB_ROWS as f32;
    let start = x0.max(0.0);
    let end = x1.min(width as f32);
    if end <= start {
        return;
    }
    let first = start.floor() as usize;
    let last = (end.ceil() as usize).min(width);
    for (offset, pixel) in row[first..last].iter_mut().enumerate() {
        let pixel_index = first + offset;
        let overlap = (end.min(pixel_index as f32 + 1.0) - start.max(pixel_index as f32)).max(0.0);
        *pixel = (*pixel + overlap * weight).min(1.0);
    }
}

/// Stroke parameters shared by every stamped piece.
struct StrokeStyle {
    width: f32,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f32,
}

/// Stamps the stroke outline of a flattened polyline into the coverage buffer.
fn stamp_stroke(
    subpaths: &[SubPath],
    style: StrokeStyle,
    canvas_width: usize,
    canvas_height: usize,
    coverage: &mut [f32],
) {
    let StrokeStyle {
        width,
        cap,
        join,
        miter_limit,
    } = style;
    let half = width * 0.5;
    if half <= 0.0 || !half.is_finite() {
        return;
    }
    for subpath in subpaths {
        let mut points: Vec<(f32, f32)> = subpath.points.clone();
        if subpath.closed && points.first() == points.last() && points.len() > 1 {
            points.pop();
        }
        if points.len() < 2 {
            continue;
        }
        let closed = subpath.closed;
        let segment_count = if closed {
            points.len()
        } else {
            points.len() - 1
        };
        // Segment bodies.
        for index in 0..segment_count {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            let (a, b) = if cap == LineCap::Square && !closed {
                match direction(a, b) {
                    Some(direction) => {
                        let extend = (direction.0 * half, direction.1 * half);
                        let is_first = index == 0;
                        let is_last = index + 1 == segment_count;
                        (
                            if is_first {
                                (a.0 - extend.0, a.1 - extend.1)
                            } else {
                                a
                            },
                            if is_last {
                                (b.0 + extend.0, b.1 + extend.1)
                            } else {
                                b
                            },
                        )
                    }
                    None => (a, b),
                }
            } else {
                (a, b)
            };
            if let Some(direction) = direction(a, b) {
                let normal = (-direction.1 * half, direction.0 * half);
                let quad = [
                    (a.0 + normal.0, a.1 + normal.1),
                    (b.0 + normal.0, b.1 + normal.1),
                    (b.0 - normal.0, b.1 - normal.1),
                    (a.0 - normal.0, a.1 - normal.1),
                ];
                stamp_polygon(&quad, canvas_width, canvas_height, coverage);
            }
        }
        // Joins.
        let join_count = if closed {
            points.len()
        } else {
            points.len().saturating_sub(2)
        };
        for index in 0..join_count {
            let previous = points[index];
            let vertex = points[(index + 1) % points.len()];
            let next = points[(index + 2) % points.len()];
            stamp_join(
                previous,
                vertex,
                next,
                half,
                join,
                miter_limit,
                canvas_width,
                canvas_height,
                coverage,
            );
        }
        // Caps on open paths.
        if !closed {
            if cap == LineCap::Round {
                if let Some(first) = points.first() {
                    stamp_disc(*first, half, canvas_width, canvas_height, coverage);
                }
                if let Some(last) = points.last() {
                    stamp_disc(*last, half, canvas_width, canvas_height, coverage);
                }
            } else if cap == LineCap::Square {
                // The extension is already part of the end segment quads.
            }
        }
    }
}

/// Stamps one line join (both sides) at `vertex`.
#[allow(clippy::too_many_arguments)]
fn stamp_join(
    previous: (f32, f32),
    vertex: (f32, f32),
    next: (f32, f32),
    half: f32,
    join: LineJoin,
    miter_limit: f32,
    width: usize,
    height: usize,
    coverage: &mut [f32],
) {
    let (Some(d0), Some(d1)) = (direction(previous, vertex), direction(vertex, next)) else {
        return;
    };
    match join {
        LineJoin::Round => stamp_disc(vertex, half, width, height, coverage),
        LineJoin::Bevel => {
            for side in [1.0f32, -1.0] {
                let n0 = (-d0.1 * half * side, d0.0 * half * side);
                let n1 = (-d1.1 * half * side, d1.0 * half * side);
                let triangle = [
                    vertex,
                    (vertex.0 + n0.0, vertex.1 + n0.1),
                    (vertex.0 + n1.0, vertex.1 + n1.1),
                ];
                stamp_polygon(&triangle, width, height, coverage);
            }
        }
        LineJoin::Miter => {
            let cross = d0.0 * d1.1 - d0.1 * d1.0;
            if cross.abs() < 1e-6 {
                return;
            }
            for side in [1.0f32, -1.0] {
                let n0 = (-d0.1 * side, d0.0 * side);
                let n1 = (-d1.1 * side, d1.0 * side);
                // Intersect v + n0*t with v + n1*u (unit normals, both at `half` distance).
                let denominator = n0.0 * n1.1 - n0.1 * n1.0;
                let tip = if denominator.abs() < 1e-6 {
                    None
                } else {
                    let t = ((n1.0 - n0.0) * n1.1 - (n1.1 - n0.1) * n1.0) / denominator;
                    let distance = (t * n0.0).hypot(t * n0.1);
                    (distance <= miter_limit * half)
                        .then_some((vertex.0 + t * n0.0 * half, vertex.1 + t * n0.1 * half))
                };
                match tip {
                    Some(tip) => {
                        let quad = [
                            vertex,
                            (vertex.0 + n0.0 * half, vertex.1 + n0.1 * half),
                            tip,
                            (vertex.0 + n1.0 * half, vertex.1 + n1.1 * half),
                        ];
                        stamp_polygon(&quad, width, height, coverage);
                    }
                    None => {
                        let triangle = [
                            vertex,
                            (vertex.0 + n0.0 * half, vertex.1 + n0.1 * half),
                            (vertex.0 + n1.0 * half, vertex.1 + n1.1 * half),
                        ];
                        stamp_polygon(&triangle, width, height, coverage);
                    }
                }
            }
        }
    }
}

/// Unit direction from `a` to `b`, or `None` for a degenerate segment.
fn direction(a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = (dx * dx + dy * dy).sqrt();
    (length > 1e-6 && length.is_finite()).then(|| (dx / length, dy / length))
}

/// Stamps a convex polygon with 4×4 supersampling, unioned with `max`.
fn stamp_polygon(points: &[(f32, f32)], width: usize, height: usize, coverage: &mut [f32]) {
    if points.len() < 3 {
        return;
    }
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &(x, y) in points {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let first_x = (min_x.floor().max(0.0) as usize).min(width);
    let last_x = ((max_x.ceil().max(0.0) as usize) + 1).min(width);
    let first_y = (min_y.floor().max(0.0) as usize).min(height);
    let last_y = ((max_y.ceil().max(0.0) as usize) + 1).min(height);
    if first_x >= last_x || first_y >= last_y {
        return;
    }
    let samples = (STAMP_SAMPLES * STAMP_SAMPLES) as f32;
    for pixel_y in first_y..last_y {
        for pixel_x in first_x..last_x {
            let mut inside = 0u32;
            for sub_y in 0..STAMP_SAMPLES {
                let y = pixel_y as f32 + (sub_y as f32 + 0.5) / STAMP_SAMPLES as f32;
                for sub_x in 0..STAMP_SAMPLES {
                    let x = pixel_x as f32 + (sub_x as f32 + 0.5) / STAMP_SAMPLES as f32;
                    if point_in_polygon(points, x, y) {
                        inside += 1;
                    }
                }
            }
            if inside > 0 {
                let index = pixel_y * width + pixel_x;
                let value = inside as f32 / samples;
                if value > coverage[index] {
                    coverage[index] = value;
                }
            }
        }
    }
}

/// Stamps a filled disc with 4×4 supersampling, unioned with `max`.
fn stamp_disc(center: (f32, f32), radius: f32, width: usize, height: usize, coverage: &mut [f32]) {
    if radius <= 0.0 || !radius.is_finite() {
        return;
    }
    let first_x = ((center.0 - radius).floor().max(0.0) as usize).min(width);
    let last_x = (((center.0 + radius).ceil().max(0.0) as usize) + 1).min(width);
    let first_y = ((center.1 - radius).floor().max(0.0) as usize).min(height);
    let last_y = (((center.1 + radius).ceil().max(0.0) as usize) + 1).min(height);
    let samples = (STAMP_SAMPLES * STAMP_SAMPLES) as f32;
    let radius_squared = radius * radius;
    for pixel_y in first_y..last_y {
        for pixel_x in first_x..last_x {
            let mut inside = 0u32;
            for sub_y in 0..STAMP_SAMPLES {
                let y = pixel_y as f32 + (sub_y as f32 + 0.5) / STAMP_SAMPLES as f32;
                for sub_x in 0..STAMP_SAMPLES {
                    let x = pixel_x as f32 + (sub_x as f32 + 0.5) / STAMP_SAMPLES as f32;
                    let dx = x - center.0;
                    let dy = y - center.1;
                    if dx * dx + dy * dy <= radius_squared {
                        inside += 1;
                    }
                }
            }
            if inside > 0 {
                let index = pixel_y * width + pixel_x;
                let value = inside as f32 / samples;
                if value > coverage[index] {
                    coverage[index] = value;
                }
            }
        }
    }
}

/// Even-odd point-in-polygon test (used for stroke primitive stamping).
fn point_in_polygon(points: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let count = points.len();
    let mut previous = count - 1;
    for current in 0..count {
        let (xi, yi) = points[current];
        let (xj, yj) = points[previous];
        if (yi > y) != (yj > y) {
            let t = (y - yi) / (yj - yi);
            let crossing = xi + t * (xj - xi);
            if x < crossing {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(width: usize, height: usize) -> Canvas {
        Canvas {
            width,
            height,
            pixels: vec![0; width * height * 4],
            coverage: vec![0.0; width * height],
        }
    }

    #[test]
    fn test_span_coverage_is_analytic() {
        let mut row = vec![0.0f32; 4];
        add_span_coverage(0.25, 2.75, 4, &mut row);
        assert_eq!(row[0], 0.1875);
        assert!((row[1] - 0.25).abs() < 1e-6);
        assert!((row[2] - 0.1875).abs() < 1e-6);
        assert_eq!(row[3], 0.0);
    }

    #[test]
    fn test_polygon_and_disc_stamps_are_antialiased() {
        let mut coverage = vec![0.0f32; 4];
        stamp_polygon(
            &[(0.5, 0.0), (2.0, 0.0), (2.0, 2.0), (0.5, 2.0)],
            2,
            2,
            &mut coverage,
        );
        assert!(coverage[0] > 0.0 && coverage[0] < 1.0);
        assert_eq!(coverage[1], 1.0);
        let mut disc = vec![0.0f32; 4];
        stamp_disc((1.0, 1.0), 0.5, 2, 2, &mut disc);
        assert!(disc[0] > 0.0);
    }

    #[test]
    fn test_composite_straight_alpha() {
        let mut canvas = canvas(1, 1);
        canvas.coverage[0] = 0.5;
        canvas.composite(Color::rgba(255, 0, 0, 255), 1.0);
        assert_eq!(canvas.pixels[3], 128);
    }
}
