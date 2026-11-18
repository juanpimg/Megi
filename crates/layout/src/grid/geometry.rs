use super::*;

pub(super) fn resolve(
    value: Length,
    calc: Option<megi_css::calc::CalcValue>,
    reference: f32,
) -> f32 {
    crate::calc::length(value, calc, reference)
}

pub(super) fn definite_length(
    value: Length,
    calc: Option<megi_css::calc::CalcValue>,
    reference: Option<f32>,
) -> Option<f32> {
    crate::calc::length_optional(value, calc, reference)
}

pub(super) fn resolve_margin(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
        top: resolve(style.margin.top, style.calc_lengths.margin_top, cb_width),
        right: resolve(
            style.margin.right,
            style.calc_lengths.margin_right,
            cb_width,
        ),
        bottom: resolve(
            style.margin.bottom,
            style.calc_lengths.margin_bottom,
            cb_width,
        ),
        left: resolve(style.margin.left, style.calc_lengths.margin_left, cb_width),
    }
}

pub(super) fn resolve_padding(style: &ComputedStyle, cb_width: f32) -> EdgeSizes {
    EdgeSizes {
        top: resolve(style.padding.top, style.calc_lengths.padding_top, cb_width),
        right: resolve(
            style.padding.right,
            style.calc_lengths.padding_right,
            cb_width,
        ),
        bottom: resolve(
            style.padding.bottom,
            style.calc_lengths.padding_bottom,
            cb_width,
        ),
        left: resolve(
            style.padding.left,
            style.calc_lengths.padding_left,
            cb_width,
        ),
    }
}

pub(super) fn horizontal_edges(style: &ComputedStyle, cb_width: f32) -> f32 {
    let padding = resolve_padding(style, cb_width);
    let margin = resolve_margin(style, cb_width);
    padding.left
        + padding.right
        + margin.left
        + margin.right
        + style.border_width.left
        + style.border_width.right
}

pub(super) fn vertical_edges(style: &ComputedStyle, cb_width: f32) -> f32 {
    let padding = resolve_padding(style, cb_width);
    let margin = resolve_margin(style, cb_width);
    padding.top
        + padding.bottom
        + margin.top
        + margin.bottom
        + style.border_width.top
        + style.border_width.bottom
}

pub(super) fn justify_track(container: &ComputedStyle, style: &ComputedStyle) -> AlignSelf {
    match style.justify_self {
        AlignSelf::Auto => match container.justify_items {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::FlexStart,
        },
        explicit => explicit,
    }
}

pub(super) fn align_track(container: &ComputedStyle, style: &ComputedStyle) -> AlignSelf {
    match style.align_self {
        AlignSelf::Auto => match container.align_items {
            AlignItems::Stretch => AlignSelf::Stretch,
            AlignItems::FlexStart => AlignSelf::FlexStart,
            AlignItems::FlexEnd => AlignSelf::FlexEnd,
            AlignItems::Center => AlignSelf::Center,
            AlignItems::Baseline => AlignSelf::FlexStart,
        },
        explicit => explicit,
    }
}

pub(super) fn offset_for(align: AlignSelf, cell: f32, size: f32) -> f32 {
    match align {
        AlignSelf::Stretch | AlignSelf::Auto | AlignSelf::Baseline | AlignSelf::FlexStart => 0.0,
        AlignSelf::FlexEnd => (cell - size).max(0.0),
        AlignSelf::Center => ((cell - size) / 2.0).max(0.0),
    }
}
