//! Public font resolution, metrics, shaping and glyph rasterization API.

use super::*;

impl FontDatabase {
    /// Rendering-facing font resolution: an unknown explicit family must fall back to the
    /// bundled default face instead of failing the whole text run (issue #1976). The strict
    /// [`FontDatabase::font`] contract is unchanged for API callers that ask for a named face.
    fn font_for_rendering(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: fontdb::Stretch,
    ) -> Result<Font, FontError> {
        match self.font(families, weight, style, stretch) {
            Ok(font) => Ok(font),
            Err(FontError::FontNotFound { .. }) => {
                let id = self
                    .last_resort_face(weight, style, stretch)
                    .ok_or(FontError::NoFontAvailable)?;
                self.build_font(id)
            }
            Err(error) => Err(error),
        }
    }

    /// Resolves a CSS family list + weight/style to a concrete font face.
    ///
    /// Unknown explicit families produce [`FontError::FontNotFound`]; generic
    /// families and the empty list use the documented last-resort face. Returns
    /// [`FontError::NoFontAvailable`] when the database is empty.
    pub fn font(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        stretch: Stretch,
    ) -> Result<Font, FontError> {
        debug_assert!(
            (1..=1000).contains(&weight),
            "font weight {weight} out of CSS range [1, 1000]"
        );
        let weight = weight.clamp(1, 1000);
        // CSS Fonts 4 §3.1: `system-ui` names the platform UI font. An `@font-face`
        // alias still wins; otherwise consult the system DB before the bundled
        // `sans-serif` fallbacks.
        if self.alias_face(families, weight, style, stretch).is_none()
            && let Some(font) = self.system_ui_font(families, weight, style, stretch)
        {
            return Ok(font);
        }
        if let Some(id) = self.query_face(families, weight, style, stretch) {
            return self.build_font(id);
        }
        // A quoted generic-like name is literal (so it must not select the generic face),
        // but if no such family is installed the browser falls back to the default face.
        let uses_generic = families.iter().any(|family| {
            GENERIC_FAMILIES.contains(&unmark_family(family).to_ascii_lowercase().as_str())
        });
        if families.is_empty() || uses_generic {
            return match self.last_resort_face(weight, style, stretch) {
                Some(id) => self.build_font(id),
                None => Err(FontError::NoFontAvailable),
            };
        }
        Err(FontError::FontNotFound {
            families: families
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        })
    }

    /// Re-resolves a previously observed [`FontRefInfo`] to a [`Font`].
    pub fn font_by_ref(&self, info: &FontRefInfo) -> Result<Font, FontError> {
        let find_in = |db: &FontDatabase| -> Option<fontdb::ID> {
            db.order
                .iter()
                .filter_map(|id| db.db.face(*id))
                .find(|face| {
                    face.post_script_name == info.post_script_name
                        && face.weight.0 == info.weight
                        && face.style == DbStyle::from(info.style)
                        && face.stretch == info.stretch
                })
                .map(|face| face.id)
        };
        // A shaped run can reference a face selected by the per-glyph system
        // fallback (CJK, Arabic, emoji). The rasterizer uses a bundled-only
        // database, so also search the shared system fallback DB before
        // giving up (#1977).
        let found = find_in(self)
            .map(|id| (self as &FontDatabase, id))
            .or_else(|| {
                if self.system_loaded {
                    return None;
                }
                let sys_db = system_fallback_db();
                find_in(sys_db).map(|id| (sys_db, id))
            })
            .ok_or_else(|| FontError::FontNotFound {
                families: info.family.clone(),
            })?;
        found.0.build_font(found.1)
    }

    /// Resolves a shaped run's font reference, consulting the system fallback database when
    /// the face came from per-glyph fallback (#1977). Otherwise the rasterizer cannot rebuild
    /// the face and silently drops those glyphs.
    pub fn font_by_ref_or_system(&self, info: &FontRefInfo) -> Result<Font, FontError> {
        if let Ok(font) = self.font_by_ref(info) {
            return Ok(font);
        }
        static SYSTEM_DB: std::sync::OnceLock<FontDatabase> = std::sync::OnceLock::new();
        let system = SYSTEM_DB.get_or_init(FontDatabase::from_system);
        system.font_by_ref(info)
    }

    /// Metrics of the resolved face at `size_px`.
    pub fn metrics(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
    ) -> Result<FontMetrics, FontError> {
        self.font(families, weight, style, Stretch::Normal)?
            .metrics(size_px)
    }

    /// Fallback-aware, unshaped text width in px at `size_px`.
    ///
    /// Sums real `hmtx` advances, choosing for each character the first face in
    /// the fallback chain that has a glyph. Kerning/ligatures are **not** applied;
    /// use [`FontDatabase::shape`] for shaped width.
    pub fn measure_text(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        text: &str,
    ) -> Result<f32, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font(families, weight, style, Stretch::Normal)?;
        let mut chain = self.fallback_chain(families, weight, style, primary);
        // Parse each face at most once per measurement; the old code re-parsed the raw
        // sfnt bytes on every glyph lookup (#1899).
        let mut resolved: HashMap<char, (usize, u16)> = HashMap::new();
        let mut extras: Vec<Font> = Vec::new();
        {
            let faces: Vec<Option<ttf_parser::Face<'_>>> =
                chain.iter().map(|font| font.ttf_face().ok()).collect();
            for ch in text.chars() {
                if resolved.contains_key(&ch) {
                    continue;
                }
                if ch.is_control() {
                    // Line breaks and other control characters have no visible glyph:
                    // scanning every system font for one cost seconds on real sites (#1899).
                    // Ordinary spaces keep their real advance from the face chain.
                    resolved.insert(ch, (0, 0));
                    continue;
                }
                let mut found = None;
                for (index, face) in faces.iter().enumerate() {
                    if let Some(gid) = face.as_ref().and_then(|face| face.glyph_index(ch)) {
                        found = Some((index, gid.0));
                        break;
                    }
                }
                if let Some(hit) = found {
                    resolved.insert(ch, hit);
                } else if let Some((font, gid)) =
                    self.find_fallback_face_for_char(ch, &chain, weight, style)
                {
                    let extra = extras.iter().position(|existing| *existing == font);
                    let position = match extra {
                        Some(position) => position,
                        None => {
                            extras.push(font);
                            extras.len() - 1
                        }
                    };
                    resolved.insert(ch, (chain.len() + position, gid));
                } else {
                    resolved.insert(ch, (0, 0));
                }
            }
        }
        chain.extend(extras);
        let faces: Vec<Option<ttf_parser::Face<'_>>> =
            chain.iter().map(|font| font.ttf_face().ok()).collect();
        let mut width = 0.0f32;
        for ch in text.chars() {
            let (index, gid) = resolved[&ch];
            let advance = faces
                .get(index)
                .and_then(|face| face.as_ref())
                .and_then(|face| face.glyph_hor_advance(ttf_parser::GlyphId(gid)))
                .unwrap_or(0) as f32;
            width += advance * (size_px / f32::from(chain[index].units_per_em.max(1)));
        }
        Ok(width)
    }

    /// Full OpenType shaping with per-glyph font fallback.
    ///
    /// Consecutive characters that resolve to different fonts are shaped as
    /// separate segments (kerning/ligatures do not cross a fallback boundary —
    /// documented limitation). Only LTR horizontal text is supported in this
    /// phase; bidi reordering is not applied.
    pub fn shape(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        text: &str,
    ) -> Result<ShapedRun, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font_for_rendering(families, weight, style, Stretch::Normal)?;
        let metrics = primary.metrics(size_px)?;
        let mut chain = self.fallback_chain(families, weight, style, primary);
        let mut fonts: Vec<FontRefInfo> = chain.iter().map(|font| font.info().clone()).collect();

        if text.is_empty() {
            return Ok(ShapedRun {
                glyphs: Vec::new(),
                width: 0.0,
                ascent: metrics.ascent,
                descent: metrics.descent,
                fonts: vec![fonts[0].clone()],
            });
        }

        let mut glyphs = Vec::new();
        let mut memo: HashMap<char, usize> = HashMap::new();
        // Current segment: (font index in `chain`, byte start, segment text).
        let mut segment: Option<(usize, usize, String)> = None;
        for (byte_index, ch) in text.char_indices() {
            let font_index = if is_combining_mark(ch)
                && let Some((current, _, _)) = &segment
                && chain[*current].has_glyph(ch)
            {
                *current
            } else {
                *memo.entry(ch).or_insert_with(|| {
                    for (index, font) in chain.iter().enumerate() {
                        if font.has_glyph(ch) {
                            return index;
                        }
                    }
                    if let Some((font, _)) =
                        self.find_fallback_face_for_char(ch, &chain, weight, style)
                    {
                        let index = chain.len();
                        fonts.push(font.info().clone());
                        chain.push(font);
                        return index;
                    }
                    0
                })
            };
            match &mut segment {
                Some((current, _start, buffer)) if *current == font_index => buffer.push(ch),
                Some((current, start, buffer)) => {
                    glyphs.extend(chain[*current].shape_segment(
                        size_px,
                        buffer,
                        *start as u32,
                        *current,
                    )?);
                    segment = Some((font_index, byte_index, ch.to_string()));
                }
                None => segment = Some((font_index, byte_index, ch.to_string())),
            }
        }
        if let Some((font_index, start, buffer)) = segment {
            glyphs.extend(chain[font_index].shape_segment(
                size_px,
                &buffer,
                start as u32,
                font_index,
            )?);
        }
        // Only emit faces that actually shaped at least one glyph, remapping the
        // glyph.font indices so unused fallback faces do not travel with the run
        // (issue #610).
        let mut used: Vec<bool> = vec![false; fonts.len()];
        for glyph in &glyphs {
            if let Some(slot) = used.get_mut(glyph.font) {
                *slot = true;
            }
        }
        // The primary face (index 0) is always retained so an empty run still
        // carries its metrics face.
        if let Some(slot) = used.first_mut() {
            *slot = true;
        }
        let mut remap: Vec<usize> = vec![0; fonts.len()];
        let mut compact: Vec<FontRefInfo> = Vec::new();
        for (index, info) in fonts.into_iter().enumerate() {
            if used[index] {
                remap[index] = compact.len();
                compact.push(info);
            }
        }
        for glyph in &mut glyphs {
            glyph.font = remap.get(glyph.font).copied().unwrap_or(0);
        }
        let width = glyphs.iter().map(|glyph| glyph.x_advance).sum();
        Ok(ShapedRun {
            glyphs,
            width,
            ascent: metrics.ascent,
            descent: metrics.descent,
            fonts: compact,
        })
    }

    /// Rasterizes a glyph of the primary face into an alpha coverage mask.
    ///
    /// `glyph_id` is interpreted in the primary face selected by the family list.
    /// For character-driven rendering with fallback use
    /// [`FontDatabase::rasterize_char`].
    pub fn rasterize_glyph(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        glyph_id: u16,
    ) -> Result<GlyphMask, FontError> {
        self.font(families, weight, style, Stretch::Normal)?
            .rasterize_glyph(size_px, u32::from(glyph_id))
    }

    /// Rasterizes the glyph for `ch`, applying per-glyph fallback.
    pub fn rasterize_char(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
        size_px: f32,
        ch: char,
    ) -> Result<GlyphMask, FontError> {
        Font::validate_size(size_px)?;
        let primary = self.font(families, weight, style, Stretch::Normal)?;
        if let Some(glyph_id) = primary.glyph_id(ch) {
            return primary.rasterize_glyph(size_px, u32::from(glyph_id));
        }
        let chain = self.fallback_chain(families, weight, style, primary);
        for font in &chain {
            if let Some(glyph_id) = font.glyph_id(ch) {
                return font.rasterize_glyph(size_px, u32::from(glyph_id));
            }
        }
        if let Some((font, glyph_id)) = self.find_fallback_face_for_char(ch, &chain, weight, style)
        {
            return font.rasterize_glyph(size_px, u32::from(glyph_id));
        }
        // No face has the glyph: render the primary face's .notdef box.
        chain[0].rasterize_glyph(size_px, 0)
    }

    /// Convenience alias kept for readability at call sites that resolve one face.
    pub fn resolve(
        &self,
        families: &[&str],
        weight: u16,
        style: FontStyle,
    ) -> Result<Font, FontError> {
        self.font(families, weight, style, Stretch::Normal)
    }
}

#[cfg(test)]
mod whitespace_fallback_tests {
    use super::*;
    use crate::font::{reset_ttf_parse_count, ttf_parse_count};

    #[test]
    fn whitespace_never_scans_the_system_fallback_fonts() {
        // #1899: measuring a text run with a newline used to scan every system font
        // looking for a glyph for '\n', taking seconds on microsoft.com.
        let db = FontDatabase::new_with_bundled_fonts();
        let families: &[&str] = &["Arial", "sans-serif"];
        reset_ttf_parse_count();
        let width = db
            .measure_text(families, 400, FontStyle::Normal, 15.0, "What's\n  new?")
            .expect("measure with newline");
        let parses = ttf_parse_count();
        assert!(width > 0.0);
        assert!(
            parses <= 16,
            "whitespace triggered {parses} font parses (expected <= 16)"
        );
    }
}
