//! Baseline, half-leading and vertical-align extent math (CSS 2.2 §10.8.1).
use super::*;

/// Half-leading box ascent/descent of a font's line box (CSS 2.2 §10.8.1).
pub(crate) fn line_box_extents(metrics: &FontMetrics) -> (f32, f32) {
    let leading = metrics.line_height - (metrics.ascent + metrics.descent);
    (
        metrics.ascent + leading / 2.0,
        metrics.descent + leading / 2.0,
    )
}

/// Applies the `vertical-align` shift to an `(ascent, descent)` pair relative to the
/// line box baseline (CSS 2.2 §10.8.1).
///
/// `top`/`bottom` do not shift the box relative to the baseline: those boxes are
/// anchored to the line box edges once the line height is known (aligned subtree).
pub(crate) fn align_extents(
    vertical_align: VerticalAlign,
    ascent: f32,
    descent: f32,
    container_font_size: f32,
    strut_ascent: f32,
    strut_descent: f32,
) -> (f32, f32) {
    match vertical_align {
        VerticalAlign::Baseline | VerticalAlign::Top | VerticalAlign::Bottom => (ascent, descent),
        VerticalAlign::Sub => {
            let shift = 0.20 * container_font_size;
            (ascent - shift, descent + shift)
        }
        VerticalAlign::Super => {
            let shift = 0.33 * container_font_size;
            (ascent + shift, descent - shift)
        }
        VerticalAlign::Length(px) => (ascent + px, descent - px),
        VerticalAlign::Middle => {
            let midpoint = 0.25 * container_font_size;
            let height = ascent + descent;
            (midpoint + height / 2.0, height / 2.0 - midpoint)
        }
        VerticalAlign::TextTop => (strut_ascent, ascent + descent - strut_ascent),
        VerticalAlign::TextBottom => (ascent + descent - strut_descent, strut_descent),
    }
}

/// Anchor of a `vertical-align: top|bottom` inline box inside the line box.
///
/// `(top, baseline, height)` are absolute layout coordinates: the top of the aligned
/// subtree, the absolute y of its baseline, and its height (ascent + descent).
pub(crate) fn anchor_for_style(
    style: &ComputedStyle,
    measurer: &dyn TextMeasurer,
    line_y: f32,
    line_height: f32,
) -> Option<(f32, f32, f32)> {
    let is_top = matches!(style.vertical_align, VerticalAlign::Top);
    let is_bottom = matches!(style.vertical_align, VerticalAlign::Bottom);
    if !is_top && !is_bottom {
        return None;
    }
    let metrics = metrics_styled(measurer, style);
    let (ascent, descent) = line_box_extents(&metrics);
    let height = ascent + descent;
    let top = if is_top {
        line_y
    } else {
        line_y + line_height - height
    };
    Some((top, top + ascent, height))
}
