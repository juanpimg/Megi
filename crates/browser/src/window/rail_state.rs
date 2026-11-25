//! Pure vertical-rail geometry shared by the window loop and headless tests.
//!
//! Keeping these helpers free of winit state lets the integration tests exercise
//! the same rail width, hit view and page reflow math the real window uses.
//!
//! Every helper has an expansion-aware `*_for` form taking both `open` and
//! `expanded`. The original one-argument helpers delegate with `expanded = true`,
//! which is the state at which the rail is first opened (Ctrl+B), so legacy
//! callers and their tests keep the exact same geometry.

use crate::chrome::{RAIL_COLLAPSED_WIDTH, RAIL_EXPANDED_WIDTH, RailView};

/// CSS width reserved by the rail for a given expanded state: zero while closed,
/// the narrow peek column while collapsed, the full title column while expanded.
#[must_use]
pub fn css_width_for(open: bool, expanded: bool) -> f32 {
    if !open {
        0.0
    } else if expanded {
        RAIL_EXPANDED_WIDTH
    } else {
        RAIL_COLLAPSED_WIDTH
    }
}

/// CSS width reserved by the expanded rail: zero while closed, so every legacy
/// pixel stays identical. Kept as the historical one-argument entry point.
#[must_use]
pub fn css_width(open: bool) -> f32 {
    css_width_for(open, true)
}

/// Flips the rail state (the transition applied by Ctrl+B and the rail toggle row).
#[must_use]
pub fn toggle(open: bool) -> bool {
    !open
}

/// Page viewport width after reserving the rail column for an expanded state.
#[must_use]
pub fn page_viewport_width_for(total_css_width: u32, open: bool, expanded: bool) -> u32 {
    total_css_width
        .saturating_sub(css_width_for(open, expanded) as u32)
        .max(1)
}

/// Page viewport width after reserving the expanded rail column.
#[must_use]
pub fn page_viewport_width(total_css_width: u32, open: bool) -> u32 {
    page_viewport_width_for(total_css_width, open, true)
}

fn safe_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Physical width of the rail column for an expanded state (`css_width_for * scale`).
#[must_use]
pub fn physical_offset_for(scale: f64, open: bool, expanded: bool) -> f64 {
    f64::from(css_width_for(open, expanded).max(0.0)) * safe_scale(scale)
}

/// Physical width of the expanded rail column.
#[must_use]
pub fn physical_offset(scale: f64, open: bool) -> f64 {
    physical_offset_for(scale, open, true)
}

/// Page CSS X for a physical pointer X after subtracting the rail column for an
/// expanded state.
#[must_use]
pub fn page_x_for(physical_x: f64, scale: f64, open: bool, expanded: bool) -> f64 {
    (physical_x - physical_offset_for(scale, open, expanded)) / safe_scale(scale)
}

/// Page CSS X after subtracting the expanded rail column.
#[must_use]
pub fn page_x(physical_x: f64, scale: f64, open: bool) -> f64 {
    page_x_for(physical_x, scale, open, true)
}

/// Builds the rail view for an expanded state, or `None` when closed.
#[must_use]
pub fn view_for<'a>(
    open: bool,
    expanded: bool,
    labels: &'a [String],
    active: usize,
    hover: Option<usize>,
) -> Option<RailView<'a>> {
    if !open {
        return None;
    }
    Some(RailView {
        labels,
        active,
        hover,
        expanded,
        width_css: css_width_for(open, expanded),
        top_css: crate::chrome::CHROME_HEIGHT,
    })
}

/// Builds the expanded rail view, or `None` when closed.
#[must_use]
pub fn view<'a>(
    open: bool,
    labels: &'a [String],
    active: usize,
    hover: Option<usize>,
) -> Option<RailView<'a>> {
    view_for(open, true, labels, active, hover)
}
