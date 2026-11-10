//! Real font metrics and text measurement for layout, backed by `crates/fonts`.
//!
//! [`FontTextMeasurer`] is the production [`TextMeasurer`] implementation: it
//! resolves the CSS `font-family` list, `font-weight` and `font-style` against a
//! [`fonts::FontDatabase`] and returns real OpenType metrics (ascent, descent,
//! line gap, x-height) and `hmtx` advances in CSS pixels.
//!
//! The deterministic [`DefaultTextMeasurer`]
//! remains available for tests and as a last-resort fallback when the font
//! database cannot resolve a face. The `measure_text_styled` /
//! `font_metrics_styled` trait methods default to the unstyled behaviour, so
//! custom measurers keep working unchanged.

use std::collections::HashMap;
use std::sync::Mutex;

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
    /// Per-measurer memoisation. The database is immutable for the lifetime of the
    /// measurer, so cached advances/metrics/family parses stay valid as long as the
    /// measurer lives (typically one layout/render pass).
    cache: Mutex<MeasurerCache>,
}

/// Maximum total text bytes kept in the advance cache before it is cleared.
const MAX_ADVANCE_CACHE_BYTES: usize = 4 * 1024 * 1024;

/// One cached font-metrics result (keyed by the CSS style fields that affect it).
#[derive(Clone, Copy)]
struct MetricsEntry {
    font_size: u32,
    line_height: Option<u32>,
    weight: u16,
    italic: bool,
    metrics: FontMetrics,
}

/// One cached text advance (keyed by text + the CSS style fields that affect it).
#[derive(Clone, Copy)]
struct AdvanceEntry {
    font_size: u32,
    weight: u16,
    italic: bool,
    width: f32,
}

/// Memoisation tables for [`FontTextMeasurer`].
#[derive(Default)]
struct MeasurerCache {
    /// Parsed CSS family lists, keyed by the computed `font-family` string.
    families: HashMap<String, Vec<String>>,
    /// Metrics per family string; the inner list has one entry per style signature.
    metrics: HashMap<String, Vec<MetricsEntry>>,
    /// Advances per text; the inner list has one entry per style signature.
    advances: HashMap<String, Vec<AdvanceEntry>>,
    /// Total `String` bytes currently stored as advance-cache keys.
    advance_bytes: usize,
}

impl FontTextMeasurer {
    /// Creates a measurer backed by the deterministic bundled fonts
    /// (`crates/fonts/assets/DejaVuSans*.ttf`), independent of system fonts.
    pub fn bundled() -> Result<Self, fonts::FontError> {
        Ok(Self {
            database: FontDatabase::load_bundled()?,
            cache: Mutex::new(MeasurerCache::default()),
        })
    }

    /// Creates a measurer from an existing font database (system fonts or custom).
    pub fn from_database(database: FontDatabase) -> Self {
        Self {
            database,
            cache: Mutex::new(MeasurerCache::default()),
        }
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
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !cache.families.contains_key(font_family) {
            let parsed = Self::families(font_family);
            cache.families.insert(font_family.to_owned(), parsed);
        }
        let size_bits = font_size.to_bits();
        if let Some(entries) = cache.advances.get(text)
            && let Some(entry) = entries.iter().find(|entry| {
                entry.font_size == size_bits
                    && entry.weight == font_weight
                    && entry.italic == font_italic
            })
        {
            return entry.width;
        }
        let families = &cache.families[font_family];
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let (weight, style) = Self::weight_and_italic(font_weight, font_italic);
        let width = match self
            .database
            .measure_text(&family_refs, weight, style, font_size, text)
        {
            Ok(width) if width.is_finite() && width >= 0.0 => width,
            // Last-resort fallback: deterministic ratio metrics, documented in
            // `DefaultTextMeasurer`. Never panics and keeps layout running even
            // with an empty/corrupt font database.
            _ => DefaultTextMeasurer.measure_text(text, font_family, font_size),
        };
        if cache.advance_bytes + text.len() > MAX_ADVANCE_CACHE_BYTES {
            cache.advances.clear();
            cache.advance_bytes = 0;
        }
        let entry = AdvanceEntry {
            font_size: size_bits,
            weight: font_weight,
            italic: font_italic,
            width,
        };
        match cache.advances.get_mut(text) {
            Some(entries) => entries.push(entry),
            None => {
                cache.advance_bytes += text.len();
                cache.advances.insert(text.to_owned(), vec![entry]);
            }
        }
        width
    }

    fn font_metrics_styled(
        &self,
        font_family: &str,
        font_size: f32,
        computed_line_height: Option<f32>,
        font_weight: u16,
        font_italic: bool,
    ) -> FontMetrics {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let size_bits = font_size.to_bits();
        let line_height_bits = computed_line_height.map(f32::to_bits);
        if let Some(entries) = cache.metrics.get(font_family)
            && let Some(entry) = entries.iter().find(|entry| {
                entry.font_size == size_bits
                    && entry.line_height == line_height_bits
                    && entry.weight == font_weight
                    && entry.italic == font_italic
            })
        {
            return entry.metrics;
        }
        if !cache.families.contains_key(font_family) {
            let parsed = Self::families(font_family);
            cache.families.insert(font_family.to_owned(), parsed);
        }
        let families = &cache.families[font_family];
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let (weight, style) = Self::weight_and_italic(font_weight, font_italic);
        let metrics = match self
            .database
            .metrics(&family_refs, weight, style, font_size)
        {
            Ok(metrics) => FontMetrics {
                font_size,
                ascent: metrics.ascent,
                descent: metrics.descent,
                x_height: metrics.x_height,
                line_height: computed_line_height.unwrap_or_else(|| metrics.line_height()),
            },
            _ => DefaultTextMeasurer.font_metrics(font_family, font_size, computed_line_height),
        };
        let entry = MetricsEntry {
            font_size: size_bits,
            line_height: line_height_bits,
            weight: font_weight,
            italic: font_italic,
            metrics,
        };
        cache
            .metrics
            .entry(font_family.to_owned())
            .or_default()
            .push(entry);
        metrics
    }
}

impl std::fmt::Debug for FontTextMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontTextMeasurer")
            .field("faces", &self.database.len())
            .finish()
    }
}
