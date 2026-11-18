//! Text cache, glyph masks and text drawing.
//! Split out of `rasterizer.rs` (Task 1.2).

use super::*;

/// Key of a cached shaped run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ShapeKey {
    pub(super) text: String,
    pub(super) family: String,
    pub(super) weight: u16,
    pub(super) italic: bool,
    pub(super) size_bits: u32,
}

/// One cached glyph mask, trimmed to its inked bounding box.
pub(super) struct GlyphMaskEntry {
    pub(super) mask: GlyphMask,
    /// Ink bounds inside the mask, exclusive upper bounds. Empty when the mask
    /// has no coverage at all (e.g. a space).
    pub(super) x0: u32,
    pub(super) y0: u32,
    pub(super) x1: u32,
    pub(super) y1: u32,
}

impl GlyphMaskEntry {
    pub(super) fn new(mask: GlyphMask) -> Self {
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

    pub(super) fn is_blank(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Maximum distinct glyph masks cached per pass (issue #656).
///
/// Keys are attacker-influenced (face, glyph id, font-size bits), so an
/// unbounded map lets a display list with many distinct text items pin memory.
/// When the cap is reached the mask map is cleared and refilled: drawing never
/// degrades, it only loses cache hits.
pub(super) const MAX_TEXT_CACHE_ENTRIES: usize = 1024;

/// Maximum distinct shaped runs cached per pass (issue #656).
pub(super) const MAX_SHAPED_RUNS: usize = 512;

/// Maximum distinct parsed font-family lists cached per pass (issue #656).
pub(super) const MAX_FAMILY_LISTS: usize = 256;

/// Maximum font faces resolved per pass (issue #656).
pub(super) const MAX_FACES: usize = 64;

/// Per-pass text cache: shaped runs, faces and antialiased glyph masks.
#[derive(Default)]
pub(super) struct TextCache {
    pub(super) faces: Vec<Font>,
    /// `post_script_name` → index into `faces` (`u32::MAX` = resolution failed).
    pub(super) face_ids: HashMap<String, u32>,
    /// `(face index, glyph id, font-size bits)` → mask.
    pub(super) masks: HashMap<(u32, u16, u32), GlyphMaskEntry>,
    pub(super) shaped: HashMap<ShapeKey, Arc<ShapedRun>>,
    pub(super) families: HashMap<String, Vec<String>>,
    pub(super) stats: RasterStats,
}

impl TextCache {
    /// Resolves (and caches) the face of a shaped-run font reference.
    pub(super) fn face_index(
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
    pub(super) fn shape(
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
    pub(super) fn mask(
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
pub(super) fn alpha_lut(color_alpha: u8) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (coverage, slot) in lut.iter_mut().enumerate() {
        *slot = ((u32::from(color_alpha) * coverage as u32 + 127) / 255) as u8;
    }
    lut
}

/// Upper bound for a rasterized glyph size, mirroring the painter's ceiling (#672).
pub(super) const MAX_RASTERIZED_FONT_SIZE: f32 = 1024.0;

/// Paints one shaped text run using (and filling) the per-pass cache.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_text_cached(
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
    pub(super) fn blend_glyph_mask(
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
pub(super) fn get_glyph_bitmap(ch: char) -> [u8; 8] {
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

// ---------------------------------------------------------------------------
// Rounded corners, box-shadow and text-shadow (R99)
// ---------------------------------------------------------------------------
/// Paints one `text-shadow` layer: glyph coverage offscreen + deterministic blur.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_text_shadow(
    pixmap: &mut Pixmap,
    cache: &mut TextCache,
    font_database: &FontDatabase,
    text: &str,
    rect: Rect,
    color: Color,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
    advance: f32,
    baseline: f32,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
    budget: &mut PaintBudget,
) {
    if color.a == 0 || text.is_empty() {
        return;
    }
    let blur = blur.clamp(0.0, MAX_SHADOW_BLUR);
    let pad = blur.ceil() as i64 + 2;
    let region = Rect::new(
        rect.x + offset_x - pad as f32,
        rect.y + offset_y - pad as f32,
        rect.width + 2.0 * pad as f32,
        rect.height + 2.0 * pad as f32,
    );
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = region.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = region.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((region.x + region.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((region.y + region.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    if (width as usize).saturating_mul(height as usize) > MAX_ROUNDED_GROUP_PIXELS {
        return;
    }
    // The RGBA buffer, the mask and the blur scratch are charged to the pass budget (#1415).
    let Some(_scratch) = budget.reserve_shadow((width as usize).saturating_mul(height as usize))
    else {
        return;
    };
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let text_rect = Rect::new(
        rect.x + offset_x - x0 as f32,
        rect.y + offset_y - y0 as f32,
        rect.width,
        rect.height,
    );
    let opaque = Color::new(255, 255, 255, 255);
    if !draw_text_cached(
        &mut buffer,
        cache,
        font_database,
        text,
        text_rect,
        opaque,
        font_size,
        font_family,
        font_weight,
        font_italic,
        baseline,
    ) {
        buffer.draw_text_run_fallback(text, text_rect, opaque, font_size, advance);
    }
    let mut mask: Vec<f32> = buffer
        .pixels
        .iter()
        .map(|pixel| f32::from(pixel.a) / 255.0)
        .collect();
    blur_mask(&mut mask, width as usize, height as usize, blur);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let alpha = mask[y * width as usize + x];
            if alpha > 0.0 {
                pixmap.blend_coverage(x0 + x as i64, y0 + y as i64, color, alpha);
            }
        }
    }
}

/// Paints a `background-clip: text` group: the nested background items are
/// rendered offscreen and composited through the glyph coverage of `runs`.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_text_mask_group(
    pixmap: &mut Pixmap,
    rect: Rect,
    runs: &[TextMaskRun],
    items: &[DisplayItem],
    cache: &mut TextCache,
    font_database: &FontDatabase,
    dx: f32,
    dy: f32,
    budget: &mut PaintBudget,
) {
    let Some(clip) = pixmap.clip_rect() else {
        return;
    };
    let x0 = rect.x.max(clip.x).floor().max(0.0) as i64;
    let y0 = rect.y.max(clip.y).floor().max(0.0) as i64;
    let x1 = ((rect.x + rect.width).min(clip.x + clip.width))
        .ceil()
        .min(pixmap.width as f32) as i64;
    let y1 = ((rect.y + rect.height).min(clip.y + clip.height))
        .ceil()
        .min(pixmap.height as f32) as i64;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    // The buffer is painted and then masked, so two full-size buffers are live at once.
    if !budget.enter_buffers(width, height, 2) {
        // Depth/total-pixel cap: paint the background without the glyph mask (documented).
        budget.with_flattened(|budget| {
            paint_items(pixmap, items, cache, font_database, dx, dy, budget)
        });
        return;
    }
    let mut buffer = Pixmap::new(width, height);
    buffer.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    paint_items(
        &mut buffer,
        items,
        cache,
        font_database,
        dx - x0 as f32,
        dy - y0 as f32,
        budget,
    );
    // Build the glyph coverage mask.
    let mut mask = Pixmap::new(width, height);
    mask.push_clip(Rect::new(0.0, 0.0, width as f32, height as f32));
    let opaque = Color::new(255, 255, 255, 255);
    for run in runs.iter().take(MAX_TEXT_MASK_RUNS) {
        let text_rect = Rect::new(
            run.rect.x + dx - x0 as f32,
            run.rect.y + dy - y0 as f32,
            run.rect.width,
            run.rect.height,
        );
        let rendered = draw_text_cached(
            &mut mask,
            cache,
            font_database,
            &run.text,
            text_rect,
            opaque,
            run.font_size,
            &run.font_family,
            run.font_weight,
            run.font_italic,
            run.baseline,
        );
        if !rendered {
            mask.draw_text_run_fallback(&run.text, text_rect, opaque, run.font_size, run.advance);
        }
    }
    // Composite the background through the mask.
    for y in 0..height {
        for x in 0..width {
            let coverage = mask.pixels[(y * width + x) as usize].a;
            if coverage == 0 {
                continue;
            }
            let src = buffer.pixels[(y * width + x) as usize];
            if src.a == 0 {
                continue;
            }
            pixmap.blend_coverage(
                x0 + i64::from(x),
                y0 + i64::from(y),
                src,
                f32::from(coverage) / 255.0,
            );
        }
    }
    budget.leave_buffers(width, height, 2);
}
