//! Per-pass text cache, glyph masks and text drawing.

#[allow(unused_imports)]
use super::*;

impl GlyphMaskEntry {
    pub(crate) fn new(mask: GlyphMask) -> Self {
        let mut x0 = mask.width;
        let mut y0 = mask.height;
        let mut x1 = 0u32;
        let mut y1 = 0u32;
        for row in 0..mask.height {
            let start = row as usize * mask.width as usize;
            for column in 0..mask.width {
                if mask.data[start + column as usize] != 0 {
                    x0 = x0.min(column);
                    x1 = x1.max(column + 1);
                    y0 = y0.min(row);
                    y1 = y1.max(row + 1);
                }
            }
        }
        Self {
            mask,
            x0,
            y0,
            x1,
            y1,
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

impl TextCache {
    /// Resolves (and caches) the face of a shaped-run font reference.
    pub(crate) fn face_index(
        &mut self,
        database: &FontDatabase,
        info: &megi_fonts::FontRefInfo,
    ) -> Option<u32> {
        if let Some(&index) = self.face_ids.get(&info.post_script_name) {
            return (index != u32::MAX).then_some(index);
        }
        // Bound both maps (issue #656). Face indices are pre-resolved per run
        // and reused for every glyph, so the face list must never be cleared
        // mid-pass; instead new faces are refused once the cap is reached and
        // the failure is cached so the font database is not re-queried.
        if self.face_ids.len() >= MAX_FACES {
            return None;
        }
        if self.faces.len() >= MAX_FACES {
            self.face_ids
                .insert(info.post_script_name.clone(), u32::MAX);
            return None;
        }
        let resolved = match database.font_by_ref(info) {
            Ok(font) => {
                let index = self.faces.len() as u32;
                self.faces.push(font);
                index
            }
            Err(_) => u32::MAX,
        };
        self.face_ids
            .insert(info.post_script_name.clone(), resolved);
        (resolved != u32::MAX).then_some(resolved)
    }

    /// Returns the cached shaped run, shaping it on first use.
    pub(crate) fn shape(
        &mut self,
        database: &FontDatabase,
        text: &str,
        font_family: &str,
        font_weight: u16,
        font_italic: bool,
        font_size: f32,
    ) -> Option<Arc<ShapedRun>> {
        let key = ShapeKey {
            text: text.to_string(),
            family: font_family.to_string(),
            weight: font_weight,
            italic: font_italic,
            size_bits: font_size.to_bits(),
        };
        if let Some(run) = self.shaped.get(&key) {
            self.stats.shaped_run_hits += 1;
            return Some(Arc::clone(run));
        }

        if self.families.len() >= MAX_FAMILY_LISTS {
            // Family lists are cheap to re-parse; drop the cache instead of
            // letting distinct family strings grow without bound (#656).
            self.families.clear();
        }
        let families = self
            .families
            .entry(font_family.to_string())
            .or_insert_with(|| megi_fonts::db::parse_font_families(font_family))
            .clone();
        let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();
        let style = if font_italic {
            FontsFontStyle::Italic
        } else {
            FontsFontStyle::Normal
        };
        let run = database
            .shape(&family_refs, font_weight, style, font_size, text)
            .ok()?;
        let run = Arc::new(run);
        self.stats.shaped_runs += 1;
        if self.shaped.len() >= MAX_SHAPED_RUNS {
            // Returned runs are `Arc`s, so clearing only drops the cache hits
            // for already-rendered text (issue #656).
            self.shaped.clear();
        }
        self.shaped.insert(key, Arc::clone(&run));
        Some(run)
    }

    /// Returns the cached glyph mask, rasterizing it on first use.
    pub(crate) fn mask(
        &mut self,
        face_index: u32,
        glyph_id: u16,
        size_bits: u32,
        font_size: f32,
    ) -> Option<&GlyphMaskEntry> {
        let key = (face_index, glyph_id, size_bits);
        if !self.masks.contains_key(&key) {
            let font = self.faces.get(face_index as usize)?;
            let mask = font.rasterize_glyph(font_size, u32::from(glyph_id)).ok()?;
            let entry = GlyphMaskEntry::new(mask);
            if entry.is_blank() {
                self.stats.blank_glyph_masks += 1;
            } else {
                self.stats.glyph_masks_rasterized += 1;
            }
            if self.masks.len() >= MAX_TEXT_CACHE_ENTRIES {
                // Clear instead of skipping the insert: a missing mask would
                // make `draw_text_cached` silently drop the glyph, so the cap
                // must cost cache hits, never ink (issue #656).
                self.masks.clear();
            }
            self.masks.insert(key, entry);
        } else {
            self.stats.glyph_mask_hits += 1;
        }
        self.masks.get(&key)
    }
}

/// Per-character alpha lookup table for a text color: `lut[coverage]` is the
/// effective source alpha of a mask texel with that coverage.
pub(crate) fn alpha_lut(color_alpha: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (coverage, slot) in lut.iter_mut().enumerate() {
        *slot = ((u32::from(color_alpha) * coverage as u32 + 127) / 255) as u8;
    }
    lut
}

/// Upper bound for a rasterized glyph size, mirroring the painter's ceiling (#672).
pub(crate) const MAX_RASTERIZED_FONT_SIZE: f32 = 1024.0;

/// Paints one shaped text run using (and filling) the per-pass cache.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text_cached(
    pixmap: &mut Pixmap,
    cache: &mut TextCache,
    database: &FontDatabase,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    baseline: f32,
) -> bool {
    if color.a == 0 || text.is_empty() {
        return true;
    }
    // Defence in depth (#672): the painter clamps at the item boundary, but a hand-built
    // display list can still hand this a hostile size, and a mask is allocated per glyph.
    if !font_size.is_finite() || font_size <= 0.0 {
        return true;
    }
    let font_size = font_size.min(MAX_RASTERIZED_FONT_SIZE);

    let Some(clip) = pixmap.clip_pixel_bounds() else {
        return true;
    };
    let Some(run) = cache.shape(
        database,
        text,
        font_family,
        font_weight,
        font_italic,
        font_size,
    ) else {
        return false;
    };
    cache.stats.text_items += 1;

    // Pre-resolve every face of the run once (usually one or two).
    let face_indices: Vec<Option<u32>> = run
        .fonts
        .iter()
        .map(|info| cache.face_index(database, info))
        .collect();
    let lut = alpha_lut(color.a);
    let size_bits = font_size.to_bits();
    let baseline_y = rect.y + baseline;
    let mut pen_x = rect.x;

    for glyph in &run.glyphs {
        let advance = glyph.x_advance;
        let Some(face_index) = face_indices.get(glyph.font).copied().flatten() else {
            pen_x += advance;
            continue;
        };
        let Ok(glyph_id) = u16::try_from(glyph.glyph_id) else {
            pen_x += advance;
            continue;
        };
        // Device origin of the glyph mask. `fonts` documents the mask top-left
        // pixel at `(pen_x + left, baseline_y + top)`, so both placement offsets
        // must be applied here (omitting them top-aligns every glyph's ink box,
        // which lifts periods and shifts text). Round once to the pixel grid.
        let mut drawn = false;
        if let Some(entry) = cache.mask(face_index, glyph_id, size_bits, font_size)
            && !entry.is_blank()
        {
            let origin_x = pen_x + glyph.x_offset + entry.mask.left;
            let origin_y = baseline_y - glyph.y_offset + entry.mask.top;
            if origin_x.is_finite() && origin_y.is_finite() {
                let origin_x = origin_x.round() as i32;
                let origin_y = origin_y.round() as i32;
                pixmap.blend_glyph_mask(entry, origin_x, origin_y, color, &lut, clip);
                drawn = true;
            }
        }
        if drawn {
            cache.stats.glyphs_drawn += 1;
        }
        pen_x += advance;
    }
    true
}

impl Pixmap {
    /// Blends one antialiased glyph mask at `(origin_x, origin_y)` device
    /// pixels, restricted to the mask's ink bounds and the clip intersection.
    pub(crate) fn blend_glyph_mask(
        &mut self,
        entry: &GlyphMaskEntry,
        origin_x: i32,
        origin_y: i32,
        color: Color,
        lut: &[u8; 256],
        clip: (i32, i32, i32, i32),
    ) {
        let mask = &entry.mask;
        // A long unbreakable run can saturate the origin on cast. Add the positive
        // ink offsets in i64 and clamp before narrowing so neither debug overflow checks
        // nor release wraparound can corrupt the destination rectangle (#1440).
        let dst_x0 = (i64::from(origin_x) + i64::from(entry.x0))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let dst_y0 = (i64::from(origin_y) + i64::from(entry.y0))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let dst_x1 = (i64::from(origin_x) + i64::from(entry.x1))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let dst_y1 = (i64::from(origin_y) + i64::from(entry.y1))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;

        let x0 = dst_x0.max(clip.0).max(0);
        let y0 = dst_y0.max(clip.1).max(0);
        let x1 = dst_x1.min(clip.2).min(self.width as i32);
        let y1 = dst_y1.min(clip.3).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let stride = self.width as usize;
        let mask_width = mask.width as usize;
        let mut source = color;
        for y in y0..y1 {
            let mask_row = (y - origin_y) as usize * mask_width;
            let row_start = mask_row + (x0 - origin_x) as usize;
            let row_end = mask_row + (x1 - origin_x) as usize;
            let pixel_row = y as usize * stride;
            for (offset, &coverage) in mask.data[row_start..row_end].iter().enumerate() {
                if coverage == 0 {
                    continue;
                }
                let alpha = u32::from(lut[coverage as usize]);
                if alpha == 0 {
                    continue;
                }
                source.a = alpha as u8;
                self.pixels[pixel_row + x0 as usize + offset] =
                    blend_over(self.pixels[pixel_row + x0 as usize + offset], source);
            }
        }
    }
}

/// Returns the process-wide bundled [`FontDatabase`] (DejaVu Sans regular and
/// bold), loading and parsing the bundled faces only once.
///
/// The Phase 4 pipeline must not depend on host-installed fonts, so the
/// database is deterministic; caching it also removes the font-file parsing
/// from every rasterization pass in the benchmark loop.
pub fn bundled_font_database() -> &'static FontDatabase {
    static DATABASE: OnceLock<FontDatabase> = OnceLock::new();
    DATABASE.get_or_init(|| match FontDatabase::load_bundled() {
        Ok(database) => database,
        Err(error) => {
            // A broken bundle used to render all text blank with no diagnostic (#1461).
            eprintln!("megi: bundled fonts failed to load: {error}; text may be blank");
            FontDatabase::default()
        }
    })
}

/// Compact built-in 8x8 font bitmaps for common characters (fallback to filled box).
pub(crate) fn get_glyph_bitmap(ch: char) -> [u8; 8] {
    match ch {
        'A' | 'a' => [
            0b00111100, 0b01100110, 0b01100110, 0b01111110, 0b01100110, 0b01100110, 0b01100110,
            0b00000000,
        ],
        'B' | 'b' => [
            0b01111100, 0b01100110, 0b01100110, 0b01111100, 0b01100110, 0b01100110, 0b01111100,
            0b00000000,
        ],
        'C' | 'c' => [
            0b00111100, 0b01100110, 0b01100000, 0b01100000, 0b01100000, 0b01100110, 0b00111100,
            0b00000000,
        ],
        'D' | 'd' => [
            0b01111000, 0b01100100, 0b01100110, 0b01100110, 0b01100110, 0b01100100, 0b01111000,
            0b00000000,
        ],
        'E' | 'e' => [
            0b01111110, 0b01100000, 0b01100000, 0b01111100, 0b01100000, 0b01100000, 0b01111110,
            0b00000000,
        ],
        'H' | 'h' => [
            0b01100110, 0b01100110, 0b01100110, 0b01111110, 0b01100110, 0b01100110, 0b01100110,
            0b00000000,
        ],
        'L' | 'l' => [
            0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01100000, 0b01111110,
            0b00000000,
        ],
        'M' | 'm' => [
            0b01100011, 0b01110111, 0b01111111, 0b01101011, 0b01100011, 0b01100011, 0b01100011,
            0b00000000,
        ],
        'O' | 'o' | '0' => [
            0b00111100, 0b01100110, 0b01100110, 0b01100110, 0b01100110, 0b01100110, 0b00111100,
            0b00000000,
        ],
        'T' | 't' => [
            0b01111110, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000,
            0b00000000,
        ],
        '-' => [
            0b00000000, 0b00000000, 0b00000000, 0b01111110, 0b00000000, 0b00000000, 0b00000000,
            0b00000000,
        ],
        '.' => [
            0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00110000, 0b00110000,
            0b00000000,
        ],
        // Default solid glyph outline
        _ => [
            0b01111110, 0b01000010, 0b01011010, 0b01011010, 0b01011010, 0b01000010, 0b01111110,
            0b00000000,
        ],
    }
}
