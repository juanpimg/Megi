//! Error type shared by the Megi font engine.

use std::fmt;

/// Errors returned by [`crate::FontDatabase`] and [`crate::Font`].
///
/// The font engine never panics on malformed input: invalid font files, missing
/// glyphs, out-of-range sizes and unknown families are all reported through this
/// type.
#[derive(Clone, Debug, PartialEq)]
pub enum FontError {
    /// The database contains no usable font face (e.g. a headless container with
    /// no fonts installed and no byte-loaded font).
    NoFontAvailable,
    /// None of the requested CSS families matched a face and no generic family
    /// was requested (so the documented last-resort fallback did not apply).
    FontNotFound { families: String },
    /// The font data could not be parsed as a supported OpenType/TrueType font.
    InvalidFont { reason: String },
    /// Reading a font file failed.
    Io { path: String, reason: String },
    /// The requested `font-size` is not a finite positive number.
    InvalidSize { size_px: f32 },
    /// The glyph id is outside the font's glyph count.
    MissingGlyph { glyph_id: u32, num_glyphs: u16 },
    /// A glyph id wider than `u16` cannot be represented by the rasterizer.
    UnsupportedGlyphId { glyph_id: u32 },
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFontAvailable => write!(f, "no font face is available in the database"),
            Self::FontNotFound { families } => {
                write!(f, "no font face matched families [{families}]")
            }
            Self::InvalidFont { reason } => write!(f, "invalid font data: {reason}"),
            Self::Io { path, reason } => write!(f, "cannot read font file {path}: {reason}"),
            Self::InvalidSize { size_px } => {
                write!(f, "font-size must be finite and > 0, got {size_px}")
            }
            Self::MissingGlyph {
                glyph_id,
                num_glyphs,
            } => write!(
                f,
                "glyph id {glyph_id} is out of range (font has {num_glyphs} glyphs)"
            ),
            Self::UnsupportedGlyphId { glyph_id } => {
                write!(f, "glyph id {glyph_id} exceeds the u16 range")
            }
        }
    }
}

impl std::error::Error for FontError {}
