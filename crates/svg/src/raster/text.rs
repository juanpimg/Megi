//! Submodule of [`super`]: `<text>`, `<textPath>` and glyph rasterization.

use super::*;
use crate::text::{TextAnchor, TextSpec};

/// Draws a `<text>` element: per-glyph advance positioning (no shaping/kerning),
/// `text-anchor`, `letter-spacing`, font size/weight/style and fill/stroke.
pub(crate) type TextPathSegment = (f32, (f32, f32), (f32, f32));

pub(crate) fn segment_at(
    segments: &[TextPathSegment],
    ends: &[f32],
    distance: f32,
) -> Option<TextPathSegment> {
    // Segments are contiguous and sorted by `start`, so the first one whose end reaches
    // `distance` is the only candidate. A linear scan restarted per glyph made a
    // 10k-glyph textPath over a 1M-segment path ~5e9 iterations (each with a sqrt).
    debug_assert_eq!(segments.len(), ends.len());
    let index = ends.partition_point(|end| *end < distance);
    let &(start, from, to) = segments.get(index)?;
    (distance >= start && distance <= *ends.get(index)?).then_some((start, from, to))
}

/// Flattens a `textPath`'s sub-paths into contiguous segments and their cumulative ends.
/// Cap on flattened segments per textPath: a pathological path with millions of
/// micro-segments must not turn glyph placement into an O(chars * segments) scan.
const MAX_TEXT_PATH_SEGMENTS: usize = 65_536;

pub(crate) fn text_path_segments(subpaths: &[SubPath]) -> (Vec<TextPathSegment>, Vec<f32>) {
    let mut segments: Vec<TextPathSegment> = Vec::new();
    let mut ends: Vec<f32> = Vec::new();
    let mut total = 0.0f32;
    'outer: for subpath in subpaths {
        for window in subpath.points.windows(2) {
            if segments.len() >= MAX_TEXT_PATH_SEGMENTS {
                break 'outer;
            }
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            if length.is_finite() && length > 0.0 {
                segments.push((total, (x0, y0), (x1, y1)));
                total += length;
                ends.push(total);
            }
        }
    }
    (segments, ends)
}

pub(crate) fn draw_text(
    canvas: &mut Canvas,
    spec: &TextSpec,
    gradients: &[GradientSpec],
    patterns: &[PatternSpec],
    viewport: Transform,
    budget: &Cell<PatternBudget>,
) -> Result<(), SvgError> {
    if spec.opacity <= 0.0 || spec.text.is_empty() {
        return Ok(());
    }
    // Charge the whole text up front (twice for textPath, which walks glyphs for both
    // natural-length measurement and placement) so a document full of texts cannot
    // bypass the global glyph cap (#432).
    let mut budget_state = budget.get();
    let passes = if spec.text_path.is_some() { 2u64 } else { 1 };
    let cost = (spec.text.chars().count() as u64).saturating_mul(passes);
    if cost > budget_state.glyphs {
        return Err(SvgError::Limit(format!(
            "SVG glyph budget of {MAX_SVG_GLYPHS} exceeded"
        )));
    }
    budget_state.glyphs -= cost;
    budget.set(budget_state);
    let Some(database) = crate::text::bundled_database() else {
        return Ok(());
    };
    let matrix = viewport.then(spec.transform);
    let scale = (matrix.a * matrix.d - matrix.b * matrix.c).abs().sqrt();
    if !scale.is_finite() || scale <= 0.0 {
        return Ok(());
    }
    let size = spec.font_size * scale;
    if !size.is_finite() || size <= 0.0 {
        return Ok(());
    }
    // #1663: an oversized effective font size exceeds the rasterizer's cap. Report it as a
    // typed limit error instead of silently painting nothing (silent truncation is a bug).
    if size > 4096.0 {
        return Err(SvgError::Limit(format!(
            "SVG text font-size {size} exceeds the 4096px cap"
        )));
    }
    let families = crate::text::families(&spec.font_family);
    let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
    let style = if spec.italic {
        megi_fonts::FontStyle::Italic
    } else {
        megi_fonts::FontStyle::Normal
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
        let (lengths, ends) = text_path_segments(&text_path.subpaths);
        let total_length = ends.last().copied().unwrap_or(0.0);
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
                let Some((start, from, to)) = segment_at(&lengths, &ends, distance) else {
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
        budget,
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
