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
        for &byte in chunk {
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
    let mut candidates: [Vec<u8>; 5] = std::array::from_fn(|_| vec![0u8; stride]);

    for row_index in 0..height as usize {
        let row = &rgba[row_index * stride..(row_index + 1) * stride];
        candidates[0].copy_from_slice(row);
        for x in 0..stride {
            let left = if x >= 4 { row[x - 4] } else { 0 };
            let above = previous[x];
            let upper_left = if x >= 4 { previous[x - 4] } else { 0 };
            candidates[1][x] = row[x].wrapping_sub(left); // Sub
            candidates[2][x] = row[x].wrapping_sub(above); // Up
            let average = ((u16::from(left) + u16::from(above)) / 2) as u8;
            candidates[3][x] = row[x].wrapping_sub(average); // Average
            candidates[4][x] = row[x].wrapping_sub(paeth_predictor(left, above, upper_left)); // Paeth
        }

        // libpng heuristic: minimise the sum of absolute signed deviations.
        let mut best_filter = 0usize;
        let mut best_score = u64::MAX;
        for (filter, candidate) in candidates.iter().enumerate() {
            let score: u64 = candidate
                .iter()
                .map(|&byte| u64::from((byte as i8).unsigned_abs()))
                .sum();
            if score < best_score {
                best_score = score;
                best_filter = filter;
            }
        }
        filtered.push(best_filter as u8);
        filtered.extend_from_slice(&candidates[best_filter]);
        previous.copy_from_slice(row);
    }
    filtered
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

    let raw = filter_scanlines(width, height, rgba);
    let mut out = Vec::with_capacity(raw.len() / 4 + 128);
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR: width, height, bit depth 8, color type 6 (RGBA), compression 0,
    // filter 0, interlace 0.
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    write_chunk(&mut out, b"IHDR", &ihdr);

    write_chunk(&mut out, b"IDAT", &deflate::zlib_compress(&raw));
    write_chunk(&mut out, b"IEND", &[]);

    out
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
