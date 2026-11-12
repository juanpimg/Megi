//! A single loaded font face: metrics, shaping and glyph rasterization.
//!
//! [`Font`] is a cheap, `Clone` handle (it shares the underlying font bytes via
//! `Arc`); layout should obtain one from [`crate::FontDatabase::font`] and keep it
//! for a whole layout pass instead of re-resolving it per text node.

use std::sync::Arc;

use ab_glyph::{Font as _, FontRef as AbFontRef, GlyphId as AbGlyphId, ScaleFont as _};
use ttf_parser::Face as TtfFace;

use crate::error::FontError;
use crate::types::{FontMetrics, FontRefInfo, GlyphMask, ShapedGlyph, ShapedRun};

/// A resolved font face bound to its raw OpenType bytes.
///
/// Created via [`crate::FontDatabase::font`] (or `font_by_ref`). All public
/// methods validate their inputs and return [`FontError`] instead of panicking.
#[derive(Clone)]
pub struct Font {
    pub(crate) data: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) info: FontRefInfo,
    pub(crate) units_per_em: u16,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("family", &self.info.family)
            .field("post_script_name", &self.info.post_script_name)
            .field("weight", &self.info.weight)
            .field("style", &self.info.style)
            .field("units_per_em", &self.units_per_em)
            .finish()
    }
}

impl PartialEq for Font {
    fn eq(&self, other: &Self) -> bool {
        self.info == other.info && Arc::ptr_eq(&self.data, &other.data)
    }
}

/// Upper bound accepted by [`Font::validate_size`] to catch absurd values
/// (e.g. `f32::MAX`) before they overflow glyph buffer allocations.
const MAX_FONT_SIZE_PX: f32 = 1_000_000.0;

/// Maximum pixels of one rasterized glyph mask.
///
/// `font-size` can be a large finite number, and a glyph's device bounds grow with it; the
/// mask (`width * height` bytes) and the rasterizer's f32 grid are allocated before the
/// destination clip is consulted, so a single glyph must be bounded here.
const MAX_GLYPH_MASK_PIXELS: usize = 16 * 1024 * 1024;

impl Font {
    /// Parses the font bytes, computes `units_per_em` and returns a handle.
    ///
    /// Used internally by [`crate::FontDatabase`]; the data is validated by the
    /// database before this constructor is reached.
    pub(crate) fn new(
        data: Arc<Vec<u8>>,
        index: u32,
        info: FontRefInfo,
    ) -> Result<Self, FontError> {
        let units_per_em = TtfFace::parse(&data, index)
            .map_err(|err| FontError::InvalidFont {
                reason: format!("ttf-parser rejected face {index}: {err}"),
            })?
            .units_per_em()
            .max(1);
        Ok(Self {
            data,
            index,
            info,
            units_per_em,
        })
    }

    /// Face identity (family, weight, style).
    pub fn info(&self) -> &FontRefInfo {
        &self.info
    }

    /// English family name of the face.
    pub fn family_name(&self) -> &str {
        &self.info.family
    }

    /// `head.unitsPerEm` of the face (1000 or 2048 for the common formats).
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Number of glyphs in the face.
    pub fn num_glyphs(&self) -> u16 {
        self.ttf_face()
            .map(|face| face.number_of_glyphs())
            .unwrap_or(0)
    }

    /// Validation shared by every size-taking method.
    pub(crate) fn validate_size(size_px: f32) -> Result<(), FontError> {
        if size_px.is_finite() && size_px > 0.0 && size_px <= MAX_FONT_SIZE_PX {
            Ok(())
        } else {
            Err(FontError::InvalidSize { size_px })
        }
    }

    pub(crate) fn ttf_face(&self) -> Result<TtfFace<'_>, FontError> {
        TtfFace::parse(&self.data, self.index).map_err(|err| FontError::InvalidFont {
            reason: format!("ttf-parser rejected face {}: {err}", self.index),
        })
    }

    fn rb_face(&self) -> Result<rustybuzz::Face<'_>, FontError> {
        rustybuzz::Face::from_slice(&self.data, self.index).ok_or_else(|| FontError::InvalidFont {
            reason: format!("rustybuzz rejected face {}", self.index),
        })
    }

    fn ab_font(&self) -> Result<AbFontRef<'_>, FontError> {
        AbFontRef::try_from_slice_and_index(&self.data, self.index).map_err(|err| {
            FontError::InvalidFont {
                reason: format!("ab_glyph rejected face {}: {err}", self.index),
            }
        })
    }

    /// Metrics of this face at `size_px`, in CSS pixels.
    ///
    /// `x_height`/`cap_height` fall back to `0.5 * size` / `0.7 * size` when the
    /// font omits `OS/2.sxHeight` / `OS/2.sCapHeight` (documented approximation).
    pub fn metrics(&self, size_px: f32) -> Result<FontMetrics, FontError> {
        Self::validate_size(size_px)?;
        let face = self.ttf_face()?;
        let upem = f32::from(face.units_per_em().max(1));
        let scale = size_px / upem;
        let ascent = f32::from(face.ascender()) * scale;
        // `descender` is negative in OpenType; expose the magnitude as positive.
        let descent = -f32::from(face.descender()) * scale;
        let line_gap = f32::from(face.line_gap()) * scale;
        let x_height = face
            .x_height()
            .map(|v| f32::from(v) * scale)
            .unwrap_or(size_px * 0.5);
        let cap_height = face
            .capital_height()
            .map(|v| f32::from(v) * scale)
            .unwrap_or(size_px * 0.7);
        Ok(FontMetrics {
            units_per_em: face.units_per_em().max(1),
            ascent,
            descent,
            line_gap,
            x_height,
            cap_height,
        })
    }

    /// Glyph index for `ch`, or `None` when the face has no glyph for it.
    pub fn glyph_id(&self, ch: char) -> Option<u16> {
        let face = self.ttf_face().ok()?;
        face.glyph_index(ch).map(|id| id.0)
    }

    /// True if the face provides a glyph for `ch`.
    pub fn has_glyph(&self, ch: char) -> bool {
        self.glyph_id(ch).is_some()
    }

    /// Horizontal advance of `glyph_id` at `size_px`, in pixels.
    ///
    /// Returns `0.0` for glyph ids without an `hmtx` entry (external glyphs).
    pub fn glyph_advance(&self, glyph_id: u16, size_px: f32) -> Result<f32, FontError> {
        Self::validate_size(size_px)?;
        let face = self.ttf_face()?;
        let scale = size_px / f32::from(self.units_per_em.max(1));
        let advance = face
            .glyph_hor_advance(ttf_parser::GlyphId(glyph_id))
            .map(f32::from)
            .unwrap_or(0.0);
        Ok(advance * scale)
    }

    /// Sum of unshaped `hmtx` advances of `text` at `size_px`.
    ///
    /// This is intentionally **unshaped**: it ignores kerning, ligatures and
    /// per-glyph fallback (see [`crate::FontDatabase::measure_text`] for the
    /// fallback-aware version and [`Font::shape`] for full shaping). It is cheap,
    /// deterministic and exactly additive per character, which layout needs as a
    /// fast first-pass estimate.
    pub fn measure_text(&self, size_px: f32, text: &str) -> Result<f32, FontError> {
        Self::validate_size(size_px)?;
        let face = self.ttf_face()?;
        let scale = size_px / f32::from(self.units_per_em.max(1));
        let notdef = face
            .glyph_hor_advance(ttf_parser::GlyphId(0))
            .map(f32::from)
            .unwrap_or(0.0);
        let mut width = 0.0f32;
        for ch in text.chars() {
            let advance = face
                .glyph_index(ch)
                .and_then(|id| face.glyph_hor_advance(id))
                .map(f32::from)
                .unwrap_or(notdef);
            width += advance * scale;
        }
        Ok(width)
    }

    /// Shapes `text` with this single face (no fallback), returning positioned
    /// glyphs in LTR/HarfBuzz visual order.
    ///
    /// Ligatures and kerning/cursive attachment come from `rustybuzz` (OpenType
    /// `GSUB`/`GPOS` + legacy `kern`). Bidi reordering and vertical text are out
    /// of scope for this phase; Arabic/Hebrew are shaped as-is with the guessed
    /// script direction.
    pub fn shape(&self, size_px: f32, text: &str) -> Result<ShapedRun, FontError> {
        Self::validate_size(size_px)?;
        let metrics = self.metrics(size_px)?;
        let mut glyphs = self.shape_segment(size_px, text, 0, 0)?;
        let width = glyphs.iter().map(|g| g.x_advance).sum();
        // `shape_segment` sets `glyph.font = 0`; keep it consistent with `fonts`.
        for glyph in &mut glyphs {
            glyph.font = 0;
        }
        Ok(ShapedRun {
            glyphs,
            width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            fonts: vec![self.info.clone()],
        })
    }

    /// Shapes one same-font substring. `cluster_base` is the UTF-8 byte offset of
    /// `segment` inside the whole run, so [`ShapedGlyph::cluster`] values remain
    /// absolute. `font_index` is written into every produced glyph.
    pub(crate) fn shape_segment(
        &self,
        size_px: f32,
        segment: &str,
        cluster_base: u32,
        font_index: usize,
    ) -> Result<Vec<ShapedGlyph>, FontError> {
        if segment.is_empty() {
            return Ok(Vec::new());
        }
        let face = self.rb_face()?;
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(segment);
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let scale = size_px / f32::from(self.units_per_em.max(1));
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        let mut glyphs = Vec::with_capacity(infos.len());
        for (info, pos) in infos.iter().zip(positions) {
            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id,
                cluster: info.cluster.saturating_add(cluster_base),
                x_advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
                y_offset: pos.y_offset as f32 * scale,
                font: font_index,
            });
        }
        Ok(glyphs)
    }

    /// Rasterizes `glyph_id` at `size_px` into an 8-bit alpha coverage mask.
    ///
    /// The mask uses **y-down pixel coordinates relative to the pen origin on the
    /// baseline**: its top-left pixel sits at `(pen_x + left, baseline_y + top)`.
    /// Glyphs without an outline (e.g. spaces) return an empty mask with the
    /// correct `advance`.
    pub fn rasterize_glyph(&self, size_px: f32, glyph_id: u16) -> Result<GlyphMask, FontError> {
        Self::validate_size(size_px)?;
        let num_glyphs = self.num_glyphs();
        if glyph_id >= num_glyphs {
            return Err(FontError::MissingGlyph {
                glyph_id: u32::from(glyph_id),
                num_glyphs,
            });
        }
        let font = self.ab_font()?;
        let scaled = font.as_scaled(size_px);
        let ab_id = AbGlyphId(glyph_id);
        let advance = scaled.h_advance(ab_id);
        let ab_glyph = ab_id.with_scale(size_px);
        let Some(outlined) = font.outline_glyph(ab_glyph) else {
            return Ok(GlyphMask {
                width: 0,
                height: 0,
                left: 0.0,
                top: 0.0,
                advance,
                data: Vec::new(),
            });
        };
        let bounds = outlined.px_bounds();
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;
        if width == 0 || height == 0 {
            return Ok(GlyphMask {
                width: 0,
                height: 0,
                left: 0.0,
                top: 0.0,
                advance,
                data: Vec::new(),
            });
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or(FontError::GlyphTooLarge { width, height })?;
        if pixels > MAX_GLYPH_MASK_PIXELS {
            return Err(FontError::GlyphTooLarge { width, height });
        }
        let mut data = vec![0u8; pixels];
        outlined.draw(|x, y, coverage| {
            let index = y as usize * width as usize + x as usize;
            if let Some(slot) = data.get_mut(index) {
                *slot = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
        Ok(GlyphMask {
            width,
            height,
            left: bounds.min.x,
            top: bounds.min.y,
            advance,
            data,
        })
    }
}
