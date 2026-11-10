//! Vertical scrolling and content-height limits (pure logic, no display).
//!
//! The window renders the **whole page** at the CSS viewport width and then
//! presents the visible region for the current scroll offset. This module owns
//! the clamped scroll state and the wheel/keyboard-to-offset conversion so it
//! can be tested without a window server.

/// CSS pixels scrolled per wheel line / arrow key.
pub const LINE_HEIGHT: u32 = 24;

/// Maximum content height rendered in one pass, in CSS pixels.
///
/// The `render` pipeline caps a single viewport at
/// [`render::MAX_VIEWPORT_DIMENSION`] (8192 px). Pages taller than that are
/// clipped with a warning (the shell advertises a 32768 px memory cap, but the
/// renderer limit is the effective one).
pub const MAX_CONTENT_HEIGHT: u32 = render::MAX_VIEWPORT_DIMENSION;

/// Clamped vertical scroll state of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollState {
    offset: u32,
    content_height: u32,
    viewport_height: u32,
}

impl ScrollState {
    /// Creates a state clamped to `[0, content_height - viewport_height]`.
    pub fn new(content_height: u32, viewport_height: u32) -> Self {
        let mut state = Self {
            offset: 0,
            content_height,
            viewport_height,
        };
        state.clamp();
        state
    }

    /// Largest valid offset for the current content/viewport sizes.
    pub fn max_offset(&self) -> u32 {
        self.content_height.saturating_sub(self.viewport_height)
    }

    /// Current scroll offset in CSS pixels.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Content height in CSS pixels.
    pub fn content_height(&self) -> u32 {
        self.content_height
    }

    /// Visible viewport height in CSS pixels.
    pub fn viewport_height(&self) -> u32 {
        self.viewport_height
    }

    /// Scroll position as an integer percentage (`0..=100`) for the window title.
    pub fn percent(&self) -> u32 {
        let max = self.max_offset();
        if max == 0 {
            100
        } else {
            ((u64::from(self.offset) * 100) / u64::from(max)) as u32
        }
    }

    /// Clamps the offset into `[0, max_offset]`; returns whether it changed.
    pub fn clamp(&mut self) -> bool {
        let clamped = self.offset.min(self.max_offset());
        let changed = clamped != self.offset;
        self.offset = clamped;
        changed
    }

    /// Updates the content height and re-clamps; returns whether the offset changed.
    pub fn set_content_height(&mut self, content_height: u32) -> bool {
        self.content_height = content_height;
        self.clamp()
    }

    /// Updates the viewport height and re-clamps; returns whether the offset changed.
    pub fn set_viewport_height(&mut self, viewport_height: u32) -> bool {
        self.viewport_height = viewport_height;
        self.clamp()
    }

    /// Scrolls by `delta` CSS pixels (positive = content moves up / go down the page).
    pub fn scroll_by(&mut self, delta: i64) -> bool {
        let new = (i64::from(self.offset) + delta).clamp(0, i64::from(self.max_offset()));
        let changed = new as u32 != self.offset;
        self.offset = new as u32;
        changed
    }

    /// Scrolls to an absolute offset (clamped).
    pub fn scroll_to(&mut self, offset: u32) -> bool {
        let new = offset.min(self.max_offset());
        let changed = new != self.offset;
        self.offset = new;
        changed
    }

    /// Scrolls up by one page (90 % of the viewport, at least one line).
    pub fn page_up(&mut self) -> bool {
        self.scroll_by(-self.page_step())
    }

    /// Scrolls down by one page (90 % of the viewport, at least one line).
    pub fn page_down(&mut self) -> bool {
        self.scroll_by(self.page_step())
    }

    /// Scrolls `lines` wheel lines (positive = user scrolls down).
    pub fn scroll_lines(&mut self, lines: i32) -> bool {
        self.scroll_by(i64::from(lines) * i64::from(LINE_HEIGHT))
    }

    /// Jumps to the top.
    pub fn home(&mut self) -> bool {
        self.scroll_to(0)
    }

    /// Jumps to the bottom.
    pub fn end(&mut self) -> bool {
        self.scroll_to(u32::MAX)
    }

    fn page_step(&self) -> i64 {
        i64::from((self.viewport_height.saturating_mul(9) / 10).max(LINE_HEIGHT))
    }
}

/// Converts a winit line delta (positive = content moves down, i.e. scroll up)
/// into a scroll offset delta (positive = go down the page).
pub fn wheel_line_delta(lines: f64) -> i64 {
    -((lines * f64::from(LINE_HEIGHT)).round() as i64)
}

/// Converts a winit pixel delta (positive = content moves down) into a scroll
/// offset delta (positive = go down the page).
pub fn wheel_pixel_delta(pixels: f64) -> i64 {
    -(pixels.round() as i64)
}

/// Clamps a layout height (CSS px, fractional) to the renderable content height.
///
/// Returns `(height, capped)`, where `capped` is `true` when the page is taller
/// than [`MAX_CONTENT_HEIGHT`].
pub fn content_height(layout_height: f32) -> (u32, bool) {
    let height = if layout_height.is_finite() {
        layout_height.ceil().max(1.0) as u64
    } else {
        1
    };
    let cap = u64::from(MAX_CONTENT_HEIGHT);
    let capped = height > cap;
    (height.min(cap) as u32, capped)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn clamps_offset_to_content_and_viewport() {
        let mut state = ScrollState::new(1000, 400);
        assert_eq!(state.max_offset(), 600);
        assert_eq!(state.offset(), 0);
        assert!(state.scroll_by(250));
        assert_eq!(state.offset(), 250);
        assert!(state.scroll_by(10_000));
        assert_eq!(state.offset(), 600);
        assert!(!state.scroll_by(50), "already at the bottom");
        assert!(state.scroll_by(-100));
        assert_eq!(state.offset(), 500);
        assert!(state.scroll_by(-10_000));
        assert_eq!(state.offset(), 0);
        assert!(!state.scroll_by(-1));
    }

    #[test]
    fn page_home_end_and_percent() {
        let mut state = ScrollState::new(2000, 800);
        assert_eq!(state.percent(), 0);
        assert!(state.page_down());
        assert_eq!(state.offset(), 720); // 90 % of 800
        assert!(state.end());
        assert_eq!(state.offset(), 1200);
        assert_eq!(state.percent(), 100);
        assert!(state.home());
        assert_eq!(state.offset(), 0);
        // Content shorter than the viewport never scrolls.
        let mut short = ScrollState::new(300, 400);
        assert_eq!(short.max_offset(), 0);
        assert!(!short.page_down());
        assert_eq!(short.percent(), 100);
    }

    #[test]
    fn wheel_deltas_are_inverted() {
        assert_eq!(wheel_line_delta(3.0), -72);
        assert_eq!(wheel_line_delta(-2.0), 48);
        assert_eq!(wheel_pixel_delta(120.0), -120);
        assert_eq!(wheel_pixel_delta(-15.0), 15);
        let mut state = ScrollState::new(1000, 400);
        assert!(state.scroll_lines(2));
        assert_eq!(state.offset(), 48);
        state.scroll_by(wheel_line_delta(1.0));
        assert_eq!(state.offset(), 24);
    }

    #[test]
    fn content_height_is_clamped_to_the_renderer_cap() {
        assert_eq!(content_height(0.0), (1, false));
        assert_eq!(content_height(600.2), (601, false));
        assert_eq!(content_height(f32::NAN), (1, false));
        assert_eq!(
            content_height(f32::INFINITY),
            (1, false),
            "non-finite heights fall back to one pixel"
        );
        let (height, capped) = content_height(9000.0);
        assert_eq!(height, MAX_CONTENT_HEIGHT);
        assert!(capped);
        let (height, capped) = content_height(MAX_CONTENT_HEIGHT as f32);
        assert_eq!(height, MAX_CONTENT_HEIGHT);
        assert!(!capped);
    }

    #[test]
    fn viewport_and_content_changes_reclamp() {
        let mut state = ScrollState::new(1000, 300);
        state.scroll_to(700);
        assert_eq!(state.offset(), 700);
        // Growing the viewport reduces the maximum offset and re-clamps.
        assert!(state.set_viewport_height(500));
        assert_eq!(state.offset(), 500);
        // Shrinking the content clamps further.
        assert!(state.set_content_height(600));
        assert_eq!(state.offset(), 100);
        assert!(!state.set_content_height(600));
    }
}
