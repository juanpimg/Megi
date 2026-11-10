//! # CSS Display List Primitives
//!
//! A [`DisplayList`] represents a flat vector of deterministic drawing commands
//! extracted from the CSS layout tree according to CSS 2.1 Appendix E painting order.
//!
//! Coordinates are CSS px in device space (y-down). The rasterizer is free to
//! antialias any edge whose coordinate is fractional: the display list only
//! carries geometry, never pixel-rounded decisions.

use std::collections::HashMap;
use std::sync::Arc;

use css::{BorderRadii, BorderStyle, BoxShadow, Color};
use dom::NodeId;
use layout::Rect;

/// Decoded replaced-element images keyed by the DOM node that references them.
///
/// Built by the integration crate (`render`: `net` fetch + `image::decode`) and
/// consumed by painting. A decoded image is straight-alpha RGBA8.
pub type ImageStore = HashMap<NodeId, Arc<image::RgbaImage>>;

/// Decoded background/embedded images keyed by their **raw CSS URL string**.
///
/// CSS keeps `background-image: url(...)` and `list-style-image: url(...)` as
/// the unresolved URL text (`ComputedStyle::background_image` /
/// `ComputedStyle::list_style_image`). The integration crate (`render`) resolves
/// each URL against the document base URL, fetches it and decodes it, then puts
/// the bitmap here under the original (unresolved) URL string so paint can look
/// it up while walking the layout tree. A URL that is absent from the map simply
/// paints nothing (backgrounds) or falls back to the default list marker.
pub type BackgroundImages = HashMap<String, Arc<image::RgbaImage>>;

/// One painted side of a CSS border box.
///
/// Widths are the computed `border-*-width` values in px; a side whose style is
/// `none` or `hidden` must have width `0` (CSS 2.2 §8.5.3: "the computed value
/// of the border width is `0` when the border style is `none` or `hidden`").
/// Painting treats a zero-width side as absent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSide {
    /// Computed border width in CSS px.
    pub width: f32,
    /// Computed border style (`solid`, `dashed`, `dotted`, `double`, ...).
    pub style: BorderStyle,
    /// Computed border color for this side.
    pub color: Color,
}

impl BorderSide {
    /// A side that paints nothing.
    pub const NONE: Self = Self {
        width: 0.0,
        style: BorderStyle::None,
        color: Color::TRANSPARENT,
    };

    /// Creates an explicitly painted side.
    pub const fn new(width: f32, style: BorderStyle, color: Color) -> Self {
        Self {
            width,
            style,
            color,
        }
    }

    /// True when this side contributes pixels: non-zero width, non-zero alpha
    /// and a style that actually paints.
    pub fn is_visible(&self) -> bool {
        self.width > 0.0
            && self.width.is_finite()
            && self.color.a > 0
            && !matches!(self.style, BorderStyle::None | BorderStyle::Hidden)
    }
}

/// Shape of a CSS list marker (CSS 2.2 §12.5, audit WP-4.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    /// Filled circle/ellipse (`list-style-type: disc`).
    Disc,
    /// Hollow circle (`list-style-type: circle`).
    Circle,
    /// Filled square (`list-style-type: square`).
    Square,
}

/// One shaped text run participating in a `background-clip: text` glyph mask.
///
/// Mirrors the font/geometry information of [`DisplayItem::Text`] without the
/// paint color: the painter collects these runs from the clipping box's
/// descendants and the rasterizer renders them as a monochrome coverage mask.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMaskRun {
    /// Text to shape for the mask.
    pub text: String,
    /// Text fragment rectangle (CSS px, top-left).
    pub rect: Rect,
    /// Font size in CSS px.
    pub font_size: f32,
    /// Font family list.
    pub font_family: String,
    /// Font weight.
    pub font_weight: u16,
    /// Italic/oblique flag.
    pub font_italic: bool,
    /// Reserved per-character advance (bitmap fallback only).
    pub advance: f32,
    /// Distance from `rect.y` to the glyph baseline.
    pub baseline: f32,
}

/// A single atomic painting instruction.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayItem {
    /// Fills a solid rectangle with a specified RGBA color.
    ///
    /// Edges are antialiased when the coordinates are fractional.
    SolidColor { rect: Rect, color: Color },
    /// Draws a rectangular border around the outer edges of `rect`.
    ///
    /// `rect` is the border box. `sides` holds the four computed border sides in
    /// CSS order: **top, right, bottom, left**. Each side carries its own width,
    /// style and color; corners are joined with a mitre (the two adjacent sides
    /// partition the corner square along the outer→inner diagonal). Styles
    /// `solid`, `dashed`, `dotted`, `double` and `none`/`hidden` are painted;
    /// `groove`/`ridge`/`inset`/`outset` fall back to `solid` for now.
    Border { rect: Rect, sides: [BorderSide; 4] },
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
    /// A thin solid rectangle for a text decoration line (underline,
    /// overline or line-through), painted with antialiased edges.
    TextDecoration { rect: Rect, color: Color },
    /// A CSS list marker (bullet/box glyph) painted with analytic coverage.
    Marker {
        rect: Rect,
        color: Color,
        kind: MarkerKind,
    },
    /// An offscreen compositing group.
    ///
    /// The nested `items` are rasterized into a transparent buffer covering
    /// `rect` and then composited over the destination with the extra `opacity`
    /// factor (CSS 2.1 §12.5 / css-color-3 §3.2: `opacity < 1` creates a
    /// stacking context). Grouping matters: overlapping children contribute
    /// once to the final alpha instead of compositing individually.
    OpacityGroup {
        rect: Rect,
        opacity: f32,
        items: Vec<DisplayItem>,
    },
    /// Draws a decoded replaced element (`<img>`) scaled into `rect`.
    ///
    /// The image is stretched to fill `rect` (`object-fit: fill`, the CSS 2.2
    /// initial behaviour); `contain`/`cover`/`none` are out of scope for Phase 4.
    /// Scaling uses bilinear filtering with clamped edge sampling and composites
    /// with straight-alpha source-over, clipped by the active clip stack.
    Image {
        rect: Rect,
        image: Arc<image::RgbaImage>,
    },
    /// Fills an antialiased rounded rectangle.
    ///
    /// Used for `background-color` when `border-radius` is authored: coverage is
    /// analytic (signed distance to the rounded box) and clipped by the active
    /// clip stack.
    RoundedRect {
        rect: Rect,
        radii: BorderRadii,
        color: Color,
    },
    /// Draws an antialiased rounded border ring (solid sides only).
    ///
    /// The painter only emits this variant when every visible side has
    /// `border-style: solid`; other styles fall back to the square `Border`
    /// inside a rounded clip group (documented limitation).
    RoundedBorder {
        rect: Rect,
        sides: [BorderSide; 4],
        radii: BorderRadii,
    },
    /// Opens an offscreen group masked by an antialiased rounded rectangle.
    ///
    /// Nested items are rendered into a transparent buffer and composited through
    /// the rounded mask, so backgrounds (color + images) are clipped to
    /// `border-radius` with antialiasing without changing every primitive.
    PushRoundedGroup { rect: Rect, radii: BorderRadii },
    /// Closes and composites the topmost rounded group.
    PopRoundedGroup,
    /// One `box-shadow` layer painted behind (outer) or above (inset) the box.
    ///
    /// Blur is a deterministic three-pass separable box blur (Gaussian
    /// approximation) with radius `blur/2`, clamped to
    /// [`MAX_SHADOW_BLUR`]/[`MAX_SHADOWS`].
    BoxShadow {
        rect: Rect,
        radii: BorderRadii,
        shadow: BoxShadow,
    },
    /// One `text-shadow` layer, painted before the corresponding glyphs.
    ///
    /// The glyph coverage is rasterized offscreen, blurred with the same
    /// deterministic box-blur kernel and composited in `color`.
    TextShadow {
        text: String,
        rect: Rect,
        color: Color,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        font_italic: bool,
        advance: f32,
        baseline: f32,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
    },
    /// Opens a group masked by the glyph coverage of `runs` (CSS
    /// `background-clip: text` / `-webkit-background-clip: text`).
    ///
    /// The nested items (the element's background color and image layers) are
    /// rendered into a transparent buffer and composited through the glyph mask,
    /// so the background shows only inside the text ink. The glyphs themselves
    /// are still painted later by their [`Text`](Self::Text) items (typically with
    /// `color: transparent`, the common "gradient text" pattern).
    PushTextMaskGroup { rect: Rect, runs: Vec<TextMaskRun> },
    /// Closes and composites the topmost text-mask group.
    PopTextMaskGroup,
    /// Pushes a rectangular clipping boundary onto the clip stack.
    PushClip { rect: Rect },
    /// Pops the top clipping boundary from the clip stack.
    PopClip,
}

impl DisplayItem {
    /// Conservative bounds of the item, used for culling.
    ///
    /// Border sides that overflow the border box (they cannot in CSS) and glyph
    /// overflow (italic/overhang) are handled by the rasterizer's own clipping;
    /// these bounds only avoid touching pixels for fully off-screen items.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::SolidColor { rect, .. }
            | Self::Border { rect, .. }
            | Self::RoundedRect { rect, .. }
            | Self::RoundedBorder { rect, .. }
            | Self::PushRoundedGroup { rect, .. }
            | Self::PushTextMaskGroup { rect, .. }
            | Self::Text { rect, .. }
            | Self::TextDecoration { rect, .. }
            | Self::Marker { rect, .. }
            | Self::Image { rect, .. }
            | Self::OpacityGroup { rect, .. } => Some(*rect),
            Self::BoxShadow { rect, shadow, .. } => {
                let blur = shadow.blur.clamp(0.0, MAX_SHADOW_BLUR);
                Some(Rect::new(
                    rect.x + shadow.offset_x.min(0.0) - blur,
                    rect.y + shadow.offset_y.min(0.0) - blur,
                    rect.width + shadow.offset_x.abs() + 2.0 * blur,
                    rect.height + shadow.offset_y.abs() + 2.0 * blur,
                ))
            }
            Self::TextShadow {
                rect,
                offset_x,
                offset_y,
                blur,
                ..
            } => {
                let blur = blur.clamp(0.0, MAX_SHADOW_BLUR);
                Some(Rect::new(
                    rect.x + offset_x.min(0.0) - blur,
                    rect.y + offset_y.min(0.0) - blur,
                    rect.width + offset_x.abs() + 2.0 * blur,
                    rect.height + offset_y.abs() + 2.0 * blur,
                ))
            }
            Self::PushClip { rect } => Some(*rect),
            Self::PopClip | Self::PopRoundedGroup | Self::PopTextMaskGroup => None,
        }
    }
}

/// Hard upper bound on the number of items a [`DisplayList`] may hold.
///
/// Painting stops appending new instructions once the list reaches this size
/// (deterministic truncation). The cap bounds memory on hostile/pathological
/// documents (WP-4.12): a real 1 MiB page produces ~25 k items, so 5 M is a
/// four-orders-of-magnitude safety margin, not a layout/quality limit.
pub const MAX_DISPLAY_LIST_ITEMS: usize = 5_000_000;

/// Hard cap on the blur radius (CSS px) honoured by the software painter.
///
/// Larger `box-shadow`/`text-shadow` blurs are clamped to this value so hostile
/// pages cannot force unbounded kernels. CSS itself allows any length.
pub const MAX_SHADOW_BLUR: f32 = 64.0;

/// Hard cap on the number of shadow layers painted per box/text run.
///
/// The CSS parser already truncates lists to 32; the painter applies the same
/// cap defensively for hand-built display lists.
pub const MAX_SHADOWS: usize = 32;

/// Hard cap on the number of text runs used to build a `background-clip: text` mask.
pub const MAX_TEXT_MASK_RUNS: usize = 4096;

/// Hard cap on the pixel area of an offscreen rounded-clip group.
///
/// A background layer clipped to a huge rounded box is rendered through an
/// offscreen RGBA buffer; beyond this area the rounded clip is skipped (the
/// square background is kept) to bound memory (documented limitation).
pub const MAX_ROUNDED_GROUP_PIXELS: usize = 16_777_216;

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
    ///
    /// Appends are silently dropped once [`MAX_DISPLAY_LIST_ITEMS`] is reached;
    /// see the constant documentation for the rationale.
    pub fn push(&mut self, item: DisplayItem) {
        if self.items.len() < MAX_DISPLAY_LIST_ITEMS {
            self.items.push(item);
        }
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
