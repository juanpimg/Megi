//! Row-cached incremental region encoder.

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------------
// Row-cached region encoder (R216)
// ---------------------------------------------------------------------------

/// One cached scanline: the unfiltered RGBA bytes and the exact filtered bytes
/// (`filter byte + row bytes`) produced by the full encoder.
struct CachedRow {
    unfiltered: Vec<u8>,
    filtered: Vec<u8>,
}

/// Incremental PNG encoder that reuses filtered scanlines from the previous
/// frame and only re-filters the rows that changed (plus the row immediately
/// below a changed row, whose PNG filter depends on its predecessor).
///
/// The DEFLATE stream still covers the whole image because PNG has one zlib
/// stream; the output is therefore **byte-identical** to
/// [`Pixmap::to_png`](crate::Pixmap::to_png) for the same pixmap. The win is
/// bounded by the filtering share of the encoder (see the benchmark rows and the
/// round report): when no filtered row changed the cached PNG is returned as-is.
#[derive(Default)]
pub struct PngRegionEncoder {
    width: u32,
    height: u32,
    rows: Vec<CachedRow>,
    last_filtered: Option<Vec<u8>>,
    last_png: Option<Vec<u8>>,
}

impl PngRegionEncoder {
    /// Creates an empty encoder (the first frame is encoded fully).
    pub fn new() -> Self {
        Self::default()
    }

    /// Encodes `pixmap`, reusing cached scanlines outside the dirty rows.
    ///
    /// `dirty_rects` are a hint: rows intersecting them are always re-filtered.
    /// Independently, every row is compared with the cached copy, so a
    /// conservative/partial hint can only cost time, never correctness — the
    /// returned bytes are always identical to `pixmap.to_png()`.
    pub fn encode_regions(&mut self, pixmap: &crate::Pixmap, dirty_rects: &[Rect]) -> Vec<u8> {
        let width = pixmap.width;
        let height = pixmap.height;
        if width == 0 || height == 0 || pixmap.pixels.len() != width as usize * height as usize {
            return Vec::new();
        }
        if self.width != width || self.height != height || self.rows.len() != height as usize {
            self.width = width;
            self.height = height;
            self.rows = Vec::with_capacity(height as usize);
            self.last_filtered = None;
            self.last_png = None;
        }
        let stride = width as usize * 4;
        let mut dirty_rows = vec![false; height as usize];
        for rect in dirty_rects {
            if !rect.x.is_finite()
                || !rect.y.is_finite()
                || !rect.width.is_finite()
                || !rect.height.is_finite()
                || rect.width <= 0.0
                || rect.height <= 0.0
            {
                continue;
            }
            // Clamp both ends: a hint below the pixmap used to produce `y0 > y1` (or
            // `y0 > height`) and panic on the slice, although the docs promise that any
            // hint can only cost time.
            let y0 = (rect.y.max(0.0).floor() as usize).min(height as usize);
            let y1 = ((rect.y + rect.height).ceil() as usize).min(height as usize);
            if y0 >= y1 {
                continue;
            }
            dirty_rows[y0..y1].fill(true);
        }

        let mut filtered = Vec::with_capacity(height as usize * (stride + 1));
        let mut row = vec![0u8; stride];
        let mut previous = vec![0u8; stride];
        let mut previous_changed = false;
        for (row_index, hint_changed) in dirty_rows.iter().copied().enumerate() {
            let start = row_index * stride;
            for (destination, pixel) in row
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(&pixmap.pixels[start / 4..start / 4 + width as usize])
            {
                destination.copy_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
            }
            let changed = match self.rows.get(row_index) {
                Some(cached) => hint_changed || cached.unfiltered != row,
                None => true,
            };
            let affected = changed || previous_changed;
            if affected || self.rows.len() <= row_index {
                let filtered_start = filtered.len();
                filter_row(&row, &previous, stride, &mut filtered);
                if row_index == self.rows.len() {
                    self.rows.push(CachedRow {
                        unfiltered: row.clone(),
                        filtered: Vec::with_capacity(stride + 1),
                    });
                }
                let cached = &mut self.rows[row_index];
                cached.unfiltered.copy_from_slice(&row);
                cached.filtered.clear();
                cached
                    .filtered
                    .extend_from_slice(&filtered[filtered_start..]);
            } else {
                filtered.extend_from_slice(&self.rows[row_index].filtered);
            }
            previous.copy_from_slice(&row);
            previous_changed = changed;
        }
        // Rows removed by a resize (unreachable: sizes are checked above).
        self.rows.truncate(height as usize);

        if self.last_filtered.as_deref() == Some(filtered.as_slice())
            && let Some(png) = &self.last_png
        {
            return png.clone();
        }
        let png = finalize_png(width, height, &filtered);
        self.last_filtered = Some(filtered);
        self.last_png = Some(png.clone());
        png
    }
}

/// Encodes a pixmap with a fresh region encoder (equivalent to `to_png()`).
///
/// This is the stateless convenience entry point; callers that redraw the same
/// canvas repeatedly should keep a [`PngRegionEncoder`] to reuse cached rows.
pub fn encode_png_regions(pixmap: &crate::Pixmap, dirty_rects: &[Rect]) -> Vec<u8> {
    PngRegionEncoder::new().encode_regions(pixmap, dirty_rects)
}
