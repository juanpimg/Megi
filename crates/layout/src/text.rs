//! Real font metrics and text measurement for layout, backed by `crates/fonts`.
//!
//! [`FontTextMeasurer`] is the production [`TextMeasurer`] implementation: it
//! resolves the CSS `font-family` list, `font-weight` and `font-style` against a
//! [`fonts::FontDatabase`] and returns real OpenType metrics (ascent, descent,
//! line gap, x-height) and `hmtx` advances in CSS pixels.
//!
//! The deterministic [`DefaultTextMeasurer`](crate::inline::DefaultTextMeasurer)
//! remains available for tests and as a last-resort fallback when the font
//! database cannot resolve a face. The `measure_text_styled` /
//! `font_metrics_styled` trait methods default to the unstyled behaviour, so
//! custom measurers keep working unchanged.

use css::{ComputedStyle, FontStyle};
use fonts::FontDatabase;

use crate::inline::{DefaultTextMeasurer, FontMetrics, TextMeasurer};

/// [`TextMeasurer`] that uses the real typographic engine in `crates/fonts`.
///
/// Create it once per layout pass (it owns a [`FontDatabase`]):
///
/// ```no_run
/// use layout::{FontTextMeasurer, Rect, layout_document_with_measurer};
/// # use std::collections::HashMap;
/// # use dom::{Document, NodeId};
/// # use css::ComputedStyle;
/// # fn main() -> Result<(), fonts::FontError> {
/// let measurer = FontTextMeasurer::bundled()?;
/// # let doc = Document::new();
/// # let styles: HashMap<NodeId, ComputedStyle> = HashMap::new();
/// // let root = layout_document_with_measurer(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0), &measurer);
/// # Ok(()) }
/// ```
pub struct FontTextMeasurer {
    database: FontDatabase,
}

impl FontTextMeasurer {
    /// Creates a measurer backed by the deterministic bundled fonts
    /// (`crates/fonts/assets/DejaVuSans*.ttf`), independent of system fonts.
    pub fn bundled() -> Result<Self, fonts::FontError> {
        Ok(Self {
            database: FontDatabase::load_bundled()?,
        })
    }

    /// Creates a measurer from an existing font database (system fonts or custom).
    pub fn from_database(database: FontDatabase) -> Self {
        Self { database }
    }

    /// Read-only access to the underlying database (paint uses it for shaping).
    pub fn database(&self) -> &FontDatabase {
        &self.database
    }

    /// Resolves a CSS family value into the owned family-name list used by `fonts`.
    fn families(font_family: &str) -> Vec<String> {
        fonts::db::parse_font_families(font_family)
    }

    fn weight_and_italic(weight: u16, italic: bool) -> (u16, fonts::FontStyle) {
        (
            weight,
            if italic {
                fonts::FontStyle::Italic
            } else {
                fonts::FontStyle::Normal
            },
        )
    }
}

/// True when a CSS computed style asks for a slanted face.
pub(crate) fn style_is_italic(style: &ComputedStyle) -> bool {
    matches!(style.font_style, FontStyle::Italic | FontStyle::Oblique)
}

impl TextMeasurer for FontTextMeasurer {
    fn measure_text(&self, text: &str, font_family: &str, font_size: f32) -> f32 {
        self.measure_text_styled(text, font_family, font_size, 400, false)
    }

    fn font_metrics(
        &self,
        font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
    ) -> FontMetrics {
        self.font_metrics_styled(font_family, font_size, computed_line_height, 400, false)
    }

    fn measure_text_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        font_italic: bool,
    ) -> f32 {
        let families = Self::families(font_family);
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let (weight, style) = Self::weight_and_italic(font_weight, font_italic);
        match self
            .database
            .measure_text(&family_refs, weight, style, font_size, text)
        {
            Ok(width) if width.is_finite() && width >= 0.0 => width,
            // Last-resort fallback: deterministic ratio metrics, documented in
            // `DefaultTextMeasurer`. Never panics and keeps layout running even
            // with an empty/corrupt font database.
            _ => DefaultTextMeasurer.measure_text(text, font_family, font_size),
        }
    }

    fn font_metrics_styled(
        &self,
        font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
        font_weight: u16,
        font_italic: bool,
    ) -> FontMetrics {
        let families = Self::families(font_family);
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let (weight, style) = Self::weight_and_italic(font_weight, font_italic);
        let line_height = computed_line_height;
        match self
            .database
            .metrics(&family_refs, weight, style, font_size)
        {
            Ok(metrics) => FontMetrics {
                font_size,
                ascent: metrics.ascent,
                descent: metrics.descent,
                x_height: metrics.x_height,
                line_height: line_height.unwrap_or_else(|| metrics.line_height()),
            },
            _ => DefaultTextMeasurer.font_metrics(font_family, font_size, line_height),
        }
    }
}

impl std::fmt::Debug for FontTextMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontTextMeasurer")
            .field("faces", &self.database.len())
            .finish()
    }
}
