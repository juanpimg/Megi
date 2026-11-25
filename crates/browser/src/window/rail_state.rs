//! Pure vertical-rail geometry shared by the window loop and headless tests.
//!
//! Keeping these helpers free of winit state lets the integration tests exercise
//! the same rail width, hit view and page reflow math the real window uses.

use crate::chrome::{RAIL_EXPANDED_WIDTH, RailView};

/// CSS width reserved by the rail: zero while closed, so every legacy pixel stays identical.
#[must_use]
pub fn css_width(open: bool) -> f32 {
    if open { RAIL_EXPANDED_WIDTH } else { 0.0 }
}

/// Flips the rail state (the transition applied by Ctrl+B and the rail toggle row).
#[must_use]
pub fn toggle(open: bool) -> bool {
    !open
}

/// Page viewport width after reserving the rail column.
#[must_use]
pub fn page_viewport_width(total_css_width: u32, open: bool) -> u32 {
    total_css_width
        .saturating_sub(css_width(open) as u32)
        .max(1)
}

fn safe_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Physical width of the rail column (`rail_css_width * scale`).
#[must_use]
pub fn physical_offset(scale: f64, open: bool) -> f64 {
    if open {
        f64::from(RAIL_EXPANDED_WIDTH) * safe_scale(scale)
    } else {
        0.0
    }
}

/// Page CSS X for a physical pointer X after subtracting the open rail column.
#[must_use]
pub fn page_x(physical_x: f64, scale: f64, open: bool) -> f64 {
    (physical_x - physical_offset(scale, open)) / safe_scale(scale)
}

/// Builds the rail view used for both layout and painting, or `None` when closed.
#[must_use]
pub fn view<'a>(
    open: bool,
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
        expanded: true,
        width_css: css_width(true),
    })
}
