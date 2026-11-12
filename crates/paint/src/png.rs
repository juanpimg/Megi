//! # Dependency-Free PNG Encoder
//!
//! Encodes an RGBA pixel buffer as a valid PNG (Portable Network Graphics) file
//! without pulling any external crate, in accordance with `docs/ARCHITECTURE.md`
//! §3.1: the core crates build only on the Rust standard library, and the PNG
//! encoder is not on the authorized-dependency list (audit decision D3).
//!
//! The generated stream is deterministic:
//!
//! - **IHDR**: 8 bits per channel, color type 6 (truecolor + alpha), no
//!   interlace, compression/filter methods 0.
//! - **IDAT**: a zlib stream (RFC 1950) produced by the in-tree DEFLATE
//!   encoder ([`crate::deflate`]): LZ77 with a 32 KiB window and fixed Huffman
//!   codes. The previous *stored* DEFLATE writer produced a 1.92 MB PNG for
//!   800×600 RGBA; real compression is required for versioned golden images.
//! - **IEND**: empty terminator chunk.
//!
//! Scanlines are filtered adaptively (the classic libpng heuristic): for every
//! row the encoder tries `None`, `Sub`, `Up`, `Average` and `Paeth`, scores each
//! candidate with the sum of absolute signed deviations and keeps the cheapest.
//! Filtered data is dominated by long zero runs, which the LZ77 match finder
//! turns into a handful of length/distance pairs.
//!
//! Every chunk carries a big-endian CRC-32 (IEEE 802.3 polynomial
//! `0xEDB8_8320`) over its type and payload.
//!
//! Typical use is through [`Pixmap::to_png`](crate::Pixmap::to_png), which
//! feeds the raw RGBA bytes to [`encode_rgba`].

use std::cell::RefCell;

use css::Color;
use layout::Rect;

use crate::deflate;

/// The 8-byte PNG magic signature (PNG spec §5.2): `\x89PNG\r\n\x1a\n`.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// CRC-32 lookup table for the IEEE polynomial, computed at compile time.
const CRC_TABLE: [u32; 256] = build_crc_table();

/// Builds the CRC-32 (IEEE 802.3) lookup table in a `const` context.
const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0u32;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

/// Feeds one byte into a running CRC-32 register.
fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        let index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
        crc = CRC_TABLE[index] ^ (crc >> 8);
    }
    crc
}

/// Computes the standardized CRC-32 of `bytes` (as used by PNG chunks).
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32_update(0xFFFF_FFFF, bytes) ^ 0xFFFF_FFFF
}

/// Computes the Adler-32 checksum of `bytes` (RFC 1950 §9).
pub fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    const CHUNK: usize = 5_552;
    let mut a = 1u32;
    let mut b = 0u32;
    for chunk in bytes.chunks(CHUNK) {
        let mut tail = chunk;
        while tail.len() >= 8 {
            // b += 8*a + 8*b0 + 7*b1 + ... + 1*b7  (adler recurrence over 8
            // bytes); computed in u64 then reduced per chunk as before.
            let block: &[u8; 8] = tail[..8].try_into().expect("slice of 8");
            let sum: u32 = block.iter().map(|&byte| u32::from(byte)).sum();
            let weighted: u32 = block
                .iter()
                .enumerate()
                .map(|(index, &byte)| (8 - index as u32) * u32::from(byte))
                .sum();
            b += 8 * a + weighted;
            a += sum;
            tail = &tail[8..];
        }
        for &byte in tail {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

/// Appends a PNG chunk (`length`, `type`, `data`, `CRC`) to `out`.
fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);

    let mut crc = crc32_update(0xFFFF_FFFF, kind);
    crc = crc32_update(crc, data);
    crc ^= 0xFFFF_FFFF;

    out.extend_from_slice(data);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// PNG `Paeth` predictor (PNG spec §6.6) for one byte position.
#[inline]
fn paeth_predictor(left: u8, above: u8, upper_left: u8) -> u8 {
    let p = i32::from(left) + i32::from(above) - i32::from(upper_left);
    let pa = (p - i32::from(left)).abs();
    let pb = (p - i32::from(above)).abs();
    let pc = (p - i32::from(upper_left)).abs();
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        above
    } else {
        upper_left
    }
}

/// Builds the filtered PNG image data (one filter byte + filtered row per
/// scanline), choosing the cheapest of the five standard PNG filters per row.
pub fn filter_scanlines(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut filtered = Vec::with_capacity(height as usize * (stride + 1));
    let mut previous = vec![0u8; stride];
    for row_index in 0..height as usize {
        let row = &rgba[row_index * stride..(row_index + 1) * stride];
        filter_row(row, previous.as_slice(), stride, &mut filtered);
        previous.copy_from_slice(row);
    }
    filtered
}

/// Encodes a [`Pixmap`](crate::Pixmap)'s pixels directly, avoiding the
/// intermediate raw-RGBA buffer used by [`encode_rgba`].
///
/// The resulting PNG is **byte-identical** to
/// `encode_rgba(width, height, &pixmap.to_rgba_bytes())`; only the number of
/// memory passes changes (rows are materialized into a small reusable buffer).
pub fn encode_pixels(width: u32, height: u32, pixels: &[Color]) -> Vec<u8> {
    let Some(expected_len) = (width as usize).checked_mul(height as usize) else {
        return Vec::new();
    };
    if width == 0 || height == 0 || pixels.len() != expected_len {
        return Vec::new();
    }
    let stride = width as usize * 4;
    ENCODE_SCRATCH.with(|scratch| {
        let scratch = &mut *scratch.borrow_mut();
        let EncodeScratch {
            filtered,
            row,
            previous,
        } = scratch;
        filtered.clear();
        filtered.reserve(height as usize * (stride + 1));
        row.clear();
        row.resize(stride, 0);
        previous.clear();
        previous.resize(stride, 0);
        for chunk in pixels.chunks(width as usize) {
            let (row_pixels, _) = row.as_chunks_mut::<4>();
            for (destination, pixel) in row_pixels.iter_mut().zip(chunk) {
                *destination = [pixel.r, pixel.g, pixel.b, pixel.a];
            }
            filter_row(row, previous.as_slice(), stride, filtered);
            previous.copy_from_slice(row);
        }
        finalize_png(width, height, filtered)
    })
}

/// Reusable row/filtered buffers for [`encode_pixels`] (R231).
#[derive(Default)]
struct EncodeScratch {
    filtered: Vec<u8>,
    row: Vec<u8>,
    previous: Vec<u8>,
}

thread_local! {
    static ENCODE_SCRATCH: RefCell<EncodeScratch> = RefCell::new(EncodeScratch::default());
}

/// Filters one scanline and appends its filter byte + bytes to `filtered`.
fn filter_row(row: &[u8], previous: &[u8], stride: usize, filtered: &mut Vec<u8>) {
    // Fast path: an identical row makes the `Up` filter all-zero. For a
    // non-zero row the heuristic then chooses `Up` without computing the
    // remaining candidates (None/Sub have positive cost), and for an all-zero
    // row `None` already has cost 0 and wins the index tie-break. Both cases
    // produce exactly the same filter byte as the general path.
    if row == previous {
        if row.iter().all(|&byte| byte == 0) {
            filtered.push(0);
        } else {
            filtered.push(2);
        }
        filtered.resize(filtered.len() + stride, 0);
        return;
    }
    // Fast path: an all-zero row always wins the tie-break with filter `None` (cost 0,
    // first in selection order), even when the previous row is not zero. Skipping the
    // five-candidate score scan is byte-safe: the emitted filter byte is still 0.
    if row.iter().all(|&byte| byte == 0) {
        filtered.push(0);
        filtered.resize(filtered.len() + stride, 0);
        return;
    }

    // General path, pass 1: candidate costs in a single scan. Each score is
    // accumulated with exactly the same per-byte operations and order as the
    // separate `*_score` helpers, so the chosen filter byte is unchanged; only
    // the number of scans over the row drops from five to one (R181).
    let (none_score, sub_score, up_score, average_score, paeth_score) =
        row_filter_scores(row, previous);
    let mut best_filter = 0usize;
    let mut best_score = none_score;
    for (filter, score) in [
        (1usize, sub_score),
        (2, up_score),
        (3, average_score),
        (4, paeth_score),
    ] {
        if score < best_score {
            best_score = score;
            best_filter = filter;
            if score == 0 {
                break;
            }
        }
    }
    let _ = best_score;

    // Pass 2: write the chosen filter byte + filtered row directly.
    filtered.push(best_filter as u8);
    let start = filtered.len();
    filtered.resize(start + stride, 0);
    apply_filter(best_filter, row, previous, &mut filtered[start..]);
}

/// Computes the five filter costs for one row in a single scan.
///
/// The per-byte contributions (including the first four bytes, where left/upper
/// neighbours are zero) are identical to the separate score helpers, so the
/// selected filter and the resulting PNG bytes do not change.
#[inline]
fn row_filter_scores(row: &[u8], previous: &[u8]) -> (u64, u64, u64, u64, u64) {
    // u32 accumulators are enough (a row of 4-byte pixels times 127 is far below
    // u32::MAX) and the split loops hoist the `x >= 4` left-pixel branch out of the
    // hot loop (R213; the summed values and therefore the chosen filter are identical).
    let mut none_score = 0u32;
    let mut sub_score = 0u32;
    let mut up_score = 0u32;
    let mut average_score = 0u32;
    let mut paeth_score = 0u32;
    let stride = row.len();
    let mut x = 0usize;
    while x < stride.min(4) {
        let byte = row[x];
        let above = previous[x];
        none_score += u32::from((byte as i8).unsigned_abs());
        sub_score += u32::from((byte as i8).unsigned_abs());
        up_score += u32::from((byte.wrapping_sub(above) as i8).unsigned_abs());
        average_score += u32::from((byte.wrapping_sub(above / 2) as i8).unsigned_abs());
        paeth_score += u32::from((byte.wrapping_sub(above) as i8).unsigned_abs());
        x += 1;
    }
    while x < stride {
        let byte = row[x];
        let left = row[x - 4];
        let above = previous[x];
        let upper_left = previous[x - 4];
        none_score += u32::from((byte as i8).unsigned_abs());
        sub_score += u32::from((byte.wrapping_sub(left) as i8).unsigned_abs());
        up_score += u32::from((byte.wrapping_sub(above) as i8).unsigned_abs());
        let average = ((u16::from(left) + u16::from(above)) / 2) as u8;
        average_score += u32::from((byte.wrapping_sub(average) as i8).unsigned_abs());
        paeth_score += u32::from(
            (byte.wrapping_sub(paeth_predictor(left, above, upper_left)) as i8).unsigned_abs(),
        );
        x += 1;
    }
    (
        u64::from(none_score),
        u64::from(sub_score),
        u64::from(up_score),
        u64::from(average_score),
        u64::from(paeth_score),
    )
}

/// Writes the filtered bytes of `filter` into `output` (same length as `row`).
#[inline]
fn apply_filter(filter: usize, row: &[u8], previous: &[u8], output: &mut [u8]) {
    let stride = row.len();
    let first = 4.min(stride);
    match filter {
        0 => output.copy_from_slice(row),
        1 => {
            output[..first].copy_from_slice(&row[..first]);
            for x in 4..stride {
                output[x] = row[x].wrapping_sub(row[x - 4]);
            }
        }
        2 => {
            for (out, (&byte, &above)) in output.iter_mut().zip(row.iter().zip(previous.iter())) {
                *out = byte.wrapping_sub(above);
            }
        }
        3 => {
            for (out, (&byte, &above)) in output
                .iter_mut()
                .zip(row.iter().zip(previous.iter()))
                .take(first)
            {
                *out = byte.wrapping_sub(above / 2);
            }
            for x in 4..stride {
                let left = row[x - 4];
                let above = previous[x];
                output[x] = row[x].wrapping_sub(((u16::from(left) + u16::from(above)) / 2) as u8);
            }
        }
        _ => {
            for (out, (&byte, &above)) in output
                .iter_mut()
                .zip(row.iter().zip(previous.iter()))
                .take(first)
            {
                *out = byte.wrapping_sub(above);
            }
            for x in 4..stride {
                let left = row[x - 4];
                let above = previous[x];
                let upper_left = previous[x - 4];
                output[x] = row[x].wrapping_sub(paeth_predictor(left, above, upper_left));
            }
        }
    }
}

/// Encodes raw `rgba` bytes (row-major, 4 bytes per pixel) as a PNG file.
///
/// Returns an empty vector when `width`/`height` are zero or when
/// `rgba.len() != width * height * 4`; the caller is expected to uphold the
/// pixel-buffer invariants (`Pixmap` always does).
pub fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let Some(expected_len) = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return Vec::new();
    };
    if width == 0 || height == 0 || rgba.len() != expected_len {
        return Vec::new();
    }

    let filtered = filter_scanlines(width, height, rgba);
    finalize_png(width, height, &filtered)
}

/// Wraps already-filtered image data into a complete PNG stream.
fn finalize_png(width: u32, height: u32, filtered: &[u8]) -> Vec<u8> {
    let compressed = deflate::zlib_compress(filtered);
    let mut out = Vec::with_capacity(compressed.len() + 128);
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR: width, height, bit depth 8, color type 6 (RGBA), compression 0,
    // filter 0, interlace 0.
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    write_chunk(&mut out, b"IHDR", &ihdr);

    write_chunk(&mut out, b"IDAT", &compressed);
    write_chunk(&mut out, b"IEND", &[]);

    out
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate::inflate_zlib;

    /// Parsed chunk: `(type, payload)`.
    type Chunk = (String, Vec<u8>);

    /// Parses a PNG stream, validating every chunk CRC, and returns the chunk
    /// list and the concatenated inflated image data.
    fn parse_png(png: &[u8]) -> (Vec<Chunk>, Vec<u8>) {
        assert!(png.starts_with(&PNG_SIGNATURE), "missing PNG signature");
        let mut pos = PNG_SIGNATURE.len();
        let mut chunks = Vec::new();
        let mut idat = Vec::new();

        loop {
            assert!(pos + 8 <= png.len(), "truncated chunk header");
            let length = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
            let kind = &png[pos + 4..pos + 8];
            let data_start = pos + 8;
            let data_end = data_start + length;
            assert!(data_end + 4 <= png.len(), "truncated chunk payload");
            let data = &png[data_start..data_end];
            let stored_crc = u32::from_be_bytes(png[data_end..data_end + 4].try_into().unwrap());

            let mut computed = crc32_update(0xFFFF_FFFF, kind);
            computed = crc32_update(computed, data);
            computed ^= 0xFFFF_FFFF;
            assert_eq!(stored_crc, computed, "bad CRC for chunk {kind:?}");

            let name = String::from_utf8_lossy(kind).into_owned();
            if name == "IDAT" {
                idat.extend_from_slice(data);
            }
            chunks.push((name, data.to_vec()));
            pos = data_end + 4;
            if kind == b"IEND" {
                break;
            }
        }
        assert_eq!(pos, png.len(), "trailing bytes after IEND");

        (
            chunks,
            inflate_zlib(&idat).expect("valid zlib/IDAT payload"),
        )
    }

    /// Reverses the PNG scanline filters of an 8-bit RGBA image.
    fn rgba_from_scanlines(raw: &[u8], width: u32, height: u32) -> Vec<u8> {
        let stride = width as usize * 4;
        let mut rgba = Vec::with_capacity(stride * height as usize);
        let mut previous = vec![0u8; stride];
        for row_index in 0..height as usize {
            let start = row_index * (stride + 1);
            let filter = raw[start];
            let mut row = raw[start + 1..start + 1 + stride].to_vec();
            for x in 0..stride {
                let left = if x >= 4 { row[x - 4] } else { 0 };
                let above = previous[x];
                let upper_left = if x >= 4 { previous[x - 4] } else { 0 };
                let predicted = match filter {
                    0 => 0,
                    1 => left,
                    2 => above,
                    3 => {
                        let sum = u16::from(left) + u16::from(above);
                        (sum / 2) as u8
                    }
                    4 => paeth_predictor(left, above, upper_left),
                    other => panic!("invalid filter {other}"),
                };
                row[x] = row[x].wrapping_add(predicted);
            }
            rgba.extend_from_slice(&row);
            previous = row;
        }
        rgba
    }

    #[test]
    fn test_crc32_and_adler32_reference_vectors() {
        // Well-known PNG CRC of the IEND chunk (type only, empty payload).
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        // RFC 1950 reference example.
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn test_png_signature_and_chunk_layout() {
        let rgba = [255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 1, 2, 3, 4];
        let png = encode_rgba(2, 2, &rgba);
        assert!(!png.is_empty());
        assert_eq!(&png[..8], &PNG_SIGNATURE);

        let (chunks, inflated) = parse_png(&png);
        let names: Vec<&str> = chunks.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["IHDR", "IDAT", "IEND"]);

        let ihdr = &chunks[0].1;
        assert_eq!(ihdr.len(), 13);
        assert_eq!(u32::from_be_bytes(ihdr[0..4].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(ihdr[4..8].try_into().unwrap()), 2);
        assert_eq!(ihdr[8], 8, "bit depth");
        assert_eq!(ihdr[9], 6, "color type RGBA");
        assert_eq!(ihdr[10], 0, "compression");
        assert_eq!(ihdr[11], 0, "filter");
        assert_eq!(ihdr[12], 0, "interlace");

        // 2 rows × (1 filter byte + 2 pixels × 4 bytes).
        assert_eq!(inflated.len(), 2 * (1 + 8));
        assert_eq!(rgba_from_scanlines(&inflated, 2, 2), rgba);
    }

    #[test]
    fn test_flat_image_compresses_well() {
        let width = 200u32;
        let height = 100u32;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[12, 200, 34, 255]);
        }
        let png = encode_rgba(width, height, &rgba);
        let (_, inflated) = parse_png(&png);
        assert_eq!(rgba_from_scanlines(&inflated, width, height), rgba);
        assert!(
            png.len() < 1024,
            "flat 200x100 image should compress below 1 KiB, got {} bytes",
            png.len()
        );
    }

    #[test]
    fn test_png_pixel_roundtrip_single_pixel() {
        let rgba = [12u8, 34, 56, 78];
        let png = encode_rgba(1, 1, &rgba);
        let (_, inflated) = parse_png(&png);
        assert_eq!(rgba_from_scanlines(&inflated, 1, 1), rgba);
    }

    #[test]
    fn test_encode_rgba_rejects_inconsistent_buffers() {
        assert!(encode_rgba(0, 0, &[]).is_empty());
        assert!(encode_rgba(2, 2, &[0u8; 15]).is_empty());
    }
}
