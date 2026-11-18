//! Submodule of [`super`]: shape drawing and scanline fill rasterization.

use super::*;

/// Draws one shape (fill + stroke) using the coverage canvas.
pub(crate) fn draw_shape(
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
    // A hostile `stroke-width` (huge but finite) would build canvas-spanning quads/discs
    // evaluated with 16 samples per pixel for every stroked shape; clamp it relative to the
    // viewport (any wider stroke already covers the canvas).
    let stroke_cap = ((canvas.width as f32).powi(2) + (canvas.height as f32).powi(2))
        .sqrt()
        .max(1.0);
    let width = (shape.stroke_width * viewport_scale).min(stroke_cap);
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
pub(crate) fn draw_shape_with_effects(
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

/// Scanline fill with 4 vertical sub-rows and analytic horizontal coverage.
/// Maximum scanline work (edges × sampled rows × sub-rows) for one fill. Past it the
/// fill samples every `step`-th pixel row and replicates that row instead of dropping
/// the shape (issue #1325), so the budget still bounds CPU work from huge paths
/// (issue #627) without turning an over-budget fill, clip or mask into a no-op.
const MAX_SCANLINE_WORK: usize = 1 << 24;

pub(crate) fn fill_scanline(
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
    // Bound the per-fill work to avoid CPU DoS (issue #627) without dropping the fill
    // (issue #1325): when one row per pixel does not fit the budget, sample every
    // `step`-th pixel row and replicate its coverage over the rows it stands for.
    let per_row_work = edges.len().saturating_mul(SUB_ROWS).max(1);
    let row_budget = (MAX_SCANLINE_WORK / per_row_work).max(1);
    let step = height.div_ceil(row_budget).max(1);
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for sample in 0..height.div_ceil(step) {
        let pixel_y = sample * step;
        let rows_end = (pixel_y + step).min(height);
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
        // Replicate the sampled row over the rows it stands for. With `step == 1` the
        // range is empty, so the exact per-pixel path is byte-for-byte unchanged.
        let row_start = pixel_y * width;
        for row in (pixel_y + 1)..rows_end {
            coverage.copy_within(row_start..row_start + width, row * width);
        }
    }
}

/// Adds horizontal span `[x0, x1)` coverage to one pixel row (`1/SUB_ROWS` weight).
pub(crate) fn add_span_coverage(x0: f32, x1: f32, width: usize, row: &mut [f32]) {
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
