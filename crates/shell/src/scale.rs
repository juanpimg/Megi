//! HiDPI window metrics: physical pixels, device scale and CSS viewport (pure logic).
//!
//! The render pipeline works in **CSS pixels**; the window and softbuffer work
//! in **physical pixels**. The HTML→PNG render therefore happens at
//! `physical / scale_factor` CSS pixels, and the resulting frame is scaled up
//! with nearest-neighbour sampling when it is copied into the window buffer
//! (see [`crate::present`]).
//!
//! At runtime the scale factor can change (moving the window between monitors);
//! the window re-renders on [`crate::window`]'s debounced path.

use render::MAX_VIEWPORT_DIMENSION;

/// Smallest accepted device scale factor.
pub const MIN_SCALE: f64 = 0.25;
/// Largest accepted device scale factor (covers 8x HiDPI displays).
pub const MAX_SCALE: f64 = 8.0;

/// Physical/window and CSS/render dimensions for one scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowMetrics {
    /// Window buffer width in physical pixels.
    pub physical_width: u32,
    /// Window buffer height in physical pixels.
    pub physical_height: u32,
    /// Render width in CSS pixels (`physical_width / scale`, clamped to the render cap).
    pub css_width: u32,
    /// Render height in CSS pixels (`physical_height / scale`, clamped to the render cap).
    pub css_height: u32,
    /// Device scale factor actually used (sanitized input).
    pub scale: f64,
}

/// Sanitizes a device scale factor (non-finite/zero values fall back to 1.0 and
/// out-of-range values are clamped to [`MIN_SCALE`]..=[`MAX_SCALE`]).
pub fn sanitize_scale(scale_factor: f64) -> f64 {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        1.0
    } else {
        scale_factor.clamp(MIN_SCALE, MAX_SCALE)
    }
}

/// Converts logical (CSS) pixels to physical pixels for `scale`.
pub fn logical_to_physical(logical: u32, scale: f64) -> u32 {
    let scale = sanitize_scale(scale);
    ((f64::from(logical) * scale).round() as u32).max(1)
}

/// Converts physical pixels to logical (CSS) pixels for `scale`.
pub fn physical_to_logical(physical: u32, scale: f64) -> u32 {
    let scale = sanitize_scale(scale);
    ((f64::from(physical) / scale).round() as u32).max(1)
}

/// Computes physical/CSS dimensions for a window of `physical_width x
/// physical_height` at `scale_factor`.
///
/// The CSS viewport is clamped to `1..=render::MAX_VIEWPORT_DIMENSION` so an
/// absurd window size or a very small scale can never request an invalid render.
pub fn window_metrics(
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
) -> WindowMetrics {
    let scale = sanitize_scale(scale_factor);
    let physical_width = physical_width.max(1);
    let physical_height = physical_height.max(1);
    let css_width = physical_to_logical(physical_width, scale).min(MAX_VIEWPORT_DIMENSION);
    let css_height = physical_to_logical(physical_height, scale).min(MAX_VIEWPORT_DIMENSION);
    WindowMetrics {
        physical_width,
        physical_height,
        css_width,
        css_height,
        scale,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn scale_one_is_identity() {
        let metrics = window_metrics(800, 600, 1.0);
        assert_eq!((metrics.css_width, metrics.css_height), (800, 600));
        assert_eq!(metrics.physical_width, 800);
        assert_eq!(metrics.physical_height, 600);
        assert_eq!(logical_to_physical(800, 1.0), 800);
        assert_eq!(physical_to_logical(800, 1.0), 800);
    }

    #[test]
    fn hidpi_divides_physical_by_scale() {
        let metrics = window_metrics(1600, 1200, 2.0);
        assert_eq!((metrics.css_width, metrics.css_height), (800, 600));
        assert_eq!(logical_to_physical(800, 2.0), 1600);
        assert_eq!(physical_to_logical(1600, 2.0), 800);
    }

    #[test]
    fn fractional_scales_round_to_nearest_pixel() {
        let metrics = window_metrics(1100, 700, 1.5);
        assert_eq!(metrics.css_width, 733); // 1100 / 1.5 = 733.33
        assert_eq!(metrics.css_height, 467); // 700 / 1.5 = 466.67
        assert_eq!(logical_to_physical(733, 1.5), 1100);
        assert_eq!(physical_to_logical(1100, 1.5), 733);
    }

    #[test]
    fn invalid_and_out_of_range_scales_are_sanitized() {
        assert_eq!(sanitize_scale(f64::NAN), 1.0);
        assert_eq!(sanitize_scale(f64::INFINITY), 1.0);
        assert_eq!(sanitize_scale(0.0), 1.0);
        assert_eq!(sanitize_scale(-2.0), 1.0);
        assert_eq!(sanitize_scale(0.01), MIN_SCALE);
        assert_eq!(sanitize_scale(100.0), MAX_SCALE);
    }

    #[test]
    fn css_viewport_is_clamped_to_the_render_cap() {
        // 8192 physical px at scale 0.5 would be 16384 CSS px > render cap.
        let metrics = window_metrics(8192, 8192, 0.5);
        assert_eq!(metrics.css_width, MAX_VIEWPORT_DIMENSION);
        assert_eq!(metrics.css_height, MAX_VIEWPORT_DIMENSION);
        // Degenerate sizes never produce a zero-sized viewport.
        let metrics = window_metrics(0, 0, 1.0);
        assert_eq!((metrics.physical_width, metrics.physical_height), (1, 1));
        assert_eq!((metrics.css_width, metrics.css_height), (1, 1));
    }
}
