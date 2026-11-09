//! Public plain-data types produced by the Megi font engine.
//!
//! All geometry is expressed in **CSS pixels** (device-independent units,
//! 1px = 1/96in at zoom 1.0). Layout and paint consume these structures; no
//! font-unit leakage is allowed outside the crate.

/// CSS `font-style` values supported by the selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// Upright face.
    #[default]
    Normal,
    /// Italic face.
    Italic,
    /// Oblique face (slanted, distinct from italic in the CSS cascade).
    Oblique,
}

impl FontStyle {
    /// Lowercase CSS keyword (`normal`, `italic`, `oblique`).
    pub const fn css_keyword(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Italic => "italic",
            Self::Oblique => "oblique",
        }
    }
}

impl From<FontStyle> for fontdb::Style {
    fn from(style: FontStyle) -> Self {
        match style {
            FontStyle::Normal => fontdb::Style::Normal,
            FontStyle::Italic => fontdb::Style::Italic,
            FontStyle::Oblique => fontdb::Style::Oblique,
        }
    }
}

impl From<fontdb::Style> for FontStyle {
    fn from(style: fontdb::Style) -> Self {
        match style {
            fontdb::Style::Normal => FontStyle::Normal,
            fontdb::Style::Italic => FontStyle::Italic,
            fontdb::Style::Oblique => FontStyle::Oblique,
        }
    }
}

/// Identity of a font face, usable to re-resolve a [`crate::Font`] from a
/// [`crate::FontDatabase`] later.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontRefInfo {
    /// Primary (English) family name, e.g. `DejaVu Sans`.
    pub family: String,
    /// PostScript name, e.g. `DejaVuSans-Bold` (unique per face file).
    pub post_script_name: String,
    /// CSS numeric font weight (`100..=900`, `400` normal, `700` bold).
    pub weight: u16,
    /// CSS font style.
    pub style: FontStyle,
    /// True when every advance is the same (`fontdb` metadata).
    pub monospaced: bool,
}

/// Vertical metrics of a face at a given `font-size`, in CSS pixels.
///
/// Baseline contract for layout/paint:
/// - `ascent` is the distance from the baseline **upwards** to the ascender line
///   (positive).
/// - `descent` is the distance from the baseline **downwards** (positive; the
///   engine stores the magnitude even though OpenType `descender` is negative).
/// - `line_gap` is the recommended additional inter-line spacing (may be 0).
/// - `line_height()` = `ascent + descent + line_gap` for a single line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    /// Font design units per em (`head.unitsPerEm`, typically 1000 or 2048).
    pub units_per_em: u16,
    /// Ascender, px above the baseline (positive).
    pub ascent: f32,
    /// Descender magnitude, px below the baseline (positive).
    pub descent: f32,
    /// Recommended line gap, px (usually `>= 0`).
    pub line_gap: f32,
    /// x-height, px (`OS/2.sxHeight`; approximated as `0.5 * size` if absent).
    pub x_height: f32,
    /// Cap height, px (`OS/2.sCapHeight`; approximated as `0.7 * size` if absent).
    pub cap_height: f32,
}

impl FontMetrics {
    /// Default line advance for one line of text: `ascent + descent + line_gap`.
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }

    /// Distance from the top of the line box to the baseline.
    ///
    /// Layout places the baseline at `line_top + ascent`.
    pub fn baseline_from_top(&self) -> f32 {
        self.ascent
    }
}

/// One positioned glyph produced by shaping.
///
/// Coordinates follow the horizontal text convention used by HarfBuzz:
/// `x_advance` moves the pen forward; `x_offset`/`y_offset` are placement
/// offsets applied before drawing, with **`y_offset` positive pointing up**.
/// Paint must flip `y_offset` to its y-down pixel space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapedGlyph {
    /// Glyph index inside the font selected for this glyph.
    pub glyph_id: u32,
    /// Byte offset (UTF-8) of the source character cluster in the shaped text.
    pub cluster: u32,
    /// Pen advance, px.
    pub x_advance: f32,
    /// Horizontal placement offset, px.
    pub x_offset: f32,
    /// Vertical placement offset, px; positive is up.
    pub y_offset: f32,
    /// Index into [`ShapedRun::fonts`] (which face produced this glyph).
    pub font: usize,
}

/// Result of shaping a text run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapedRun {
    /// Positioned glyphs in visual order (LTR only for now).
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width, px (`sum(x_advance)`).
    pub width: f32,
    /// Ascender of the primary face at the shaping size, px above baseline.
    pub ascent: f32,
    /// Descender magnitude of the primary face at the shaping size, px below baseline.
    pub descent: f32,
    /// Faces used by this run; [`ShapedGlyph::font`] indexes into this vector.
    pub fonts: Vec<FontRefInfo>,
}

/// Antialiased coverage mask of a single rasterized glyph.
///
/// `data` is row-major, `width * height` bytes, 0 = fully transparent,
/// 255 = fully covered. `left`/`top` position the mask relative to the pen
/// origin **on the baseline**, in y-down pixel coordinates:
/// the mask's top-left pixel is at `(pen_x + left, baseline_y + top)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphMask {
    /// Mask width in pixels.
    pub width: u32,
    /// Mask height in pixels.
    pub height: u32,
    /// Horizontal offset from the pen origin to the mask's left edge, px.
    pub left: f32,
    /// Vertical offset from the baseline to the mask's top edge, px (y-down).
    pub top: f32,
    /// Horizontal advance of the glyph, px.
    pub advance: f32,
    /// Alpha coverage, row-major, `width * height` bytes.
    pub data: Vec<u8>,
}

impl GlyphMask {
    /// True if the glyph produced no visible outline (e.g. a space).
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Coverage at `(x, y)` inside the mask, or `0` outside its bounds.
    pub fn coverage_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data
            .get((y as usize) * (self.width as usize) + (x as usize))
            .copied()
            .unwrap_or(0)
    }
}
