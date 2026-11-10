//! # CSS Display List Primitives
//!
//! A [`DisplayList`] represents a flat vector of deterministic drawing commands
//! extracted from the CSS layout tree according to CSS 2.1 Appendix E painting order.

use css::Color;
use layout::{EdgeSizes, Rect};

/// A single atomic painting instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayItem {
    /// Fills a solid rectangle with a specified RGBA color.
    SolidColor { rect: Rect, color: Color },
    /// Draws a rectangular border around the outer edges of `rect`.
    Border {
        rect: Rect,
        border_width: EdgeSizes,
        color: Color,
    },
    /// Renders a text run within a bounding box using the real font engine.
    ///
    /// `baseline` is the distance from `rect.y` (top of the text fragment) down
    /// to the glyph baseline, propagated by the IFC for every line fragment.
    /// `advance` is the per-character advance layout reserved for this run
    /// (`rect.width / char_count`); the real rasterizer uses the shaped glyph
    /// advances and keeps `advance` for diagnostics/fallback.
    Text {
        text: String,
        rect: Rect,
        color: Color,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        font_italic: bool,
        advance: f32,
        /// Distance from `rect.y` to the glyph baseline, in px.
        baseline: f32,
    },
    /// Pushes a rectangular clipping boundary onto the clip stack.
    PushClip { rect: Rect },
    /// Pops the top clipping boundary from the clip stack.
    PopClip,
}

/// A sequential list of [`DisplayItem`]s ready for 2D software or hardware rasterization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
}

impl DisplayList {
    /// Creates a new, empty display list.
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Appends a new display item to the list.
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// Returns the number of display items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns true if the display list contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns an iterator over the display items.
    pub fn iter(&self) -> std::slice::Iter<'_, DisplayItem> {
        self.items.iter()
    }
}

impl<'a> IntoIterator for &'a DisplayList {
    type Item = &'a DisplayItem;
    type IntoIter = std::slice::Iter<'a, DisplayItem>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for DisplayList {
    type Item = DisplayItem;
    type IntoIter = std::vec::IntoIter<DisplayItem>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
