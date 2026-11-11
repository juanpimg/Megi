//! Basic SVG `<text>` support: font selection through the workspace `fonts` crate
//! (bundled deterministic DejaVu Sans, SIL/OFL compatible; the crate is an authorized
//! dependency), glyph positioning with `text-anchor`, `font-size`/`font-weight`,
//! `letter-spacing`, `dx`/`dy` and element transforms.
//!
//! Shaping is **per-glyph advance positioning** (no kerning/ligatures or bidi/Arabic
//! shaping) — documented limitation. Anti-DoS limits: 1 000 `<text>` elements and 10 000
//! characters per text.

use std::sync::OnceLock;

use crate::document::Paint;
use crate::path::SubPath;
use crate::transform::Transform;

/// Anti-DoS: maximum number of `<text>` elements in one document.
pub(crate) const MAX_TEXTS: usize = 1_000;
/// Anti-DoS: maximum number of characters per `<text>`.
pub(crate) const MAX_TEXT_CHARS: usize = 10_000;

/// Horizontal anchoring (`text-anchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum TextAnchor {
    #[default]
    Start,
    Middle,
    End,
}

impl TextAnchor {
    pub(crate) fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("middle") => Self::Middle,
            Some(value) if value.eq_ignore_ascii_case("end") => Self::End,
            _ => Self::Start,
        }
    }
}

/// Flattened path used by `<textPath>` (root user space; no rotation of glyphs).
#[derive(Clone, Debug)]
pub(crate) struct TextPathSpec {
    pub subpaths: Vec<SubPath>,
    /// `startOffset`: absolute user units (`percent == false`) or fraction of path length.
    pub start_offset: f32,
    pub percent: bool,
    /// `side="right"`: baseline shifted to the right of the path direction.
    pub side_right: bool,
}

/// One parsed `<text>` element (contents flattened; child `<tspan>` positions are ignored).
#[derive(Clone, Debug)]
pub(crate) struct TextSpec {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub anchor: TextAnchor,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub letter_spacing: f32,
    pub fill: Paint,
    pub fill_opacity: f32,
    pub stroke: Paint,
    pub stroke_opacity: f32,
    pub stroke_width: f32,
    pub opacity: f32,
    pub transform: Transform,
    /// Local bounding box (`x, y, width, height`) used by gradient `objectBoundingBox`.
    pub bbox: (f32, f32, f32, f32),
    /// Optional `<textPath>` geometry (glyphs advance along the flattened path).
    pub text_path: Option<TextPathSpec>,
    /// SVG `textLength` (user units) and whether `lengthAdjust="spacingAndGlyphs"`.
    pub text_length: Option<f32>,
    pub spacing_and_glyphs: bool,
}

impl TextSpec {
    /// Approximation of the local text box used by gradients (`x, y, width, height`).
    pub(crate) fn local_bbox(&self) -> (f32, f32, f32, f32) {
        (
            self.x,
            self.y - self.font_size,
            (self.text.chars().count() as f32) * self.font_size * 0.6,
            self.font_size * 1.2,
        )
    }
}

/// Bundled deterministic font database shared by every text render (read-only).
pub(crate) fn bundled_database() -> Option<&'static fonts::FontDatabase> {
    static DATABASE: OnceLock<Option<fonts::FontDatabase>> = OnceLock::new();
    DATABASE
        .get_or_init(|| fonts::FontDatabase::load_bundled().ok())
        .as_ref()
}

/// Splits a CSS `font-family` value into families (quotes stripped).
pub(crate) fn families(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = raw
        .split(',')
        .map(|part| {
            part.trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .trim()
                .to_string()
        })
        .filter(|part| !part.is_empty())
        .collect();
    if out.is_empty() {
        out.push("sans-serif".to_string());
    }
    out
}
