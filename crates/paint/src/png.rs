//! # Dependency-Free PNG Encoder
//!
//! Encodes an RGBA pixel buffer as a valid PNG (Portable Network Graphics) file
//! without pulling any external crate, in accordance with `docs/ARCHITECTURE.md`
//! §3.1: the core crates build only on the Rust standard library, and the PNG
//! encoder is not on the authorized-dependency list (audit decision D3).
//!
//! The generated stream is intentionally simple and fully deterministic:
//!
//! - **IHDR**: 8 bits per channel, color type 6 (truecolor + alpha), no
//!   interlace, compression/filter methods 0.
//! - **IDAT**: a zlib stream (RFC 1950) whose DEFLATE payload (RFC 1951) uses
//!   only *stored* blocks (`BTYPE=00`), so no Huffman/LZ77 machinery is
//!   required. A stored DEFLATE payload is just the raw bytes split into
//!   blocks of at most 65 535 bytes, prefixed with `BFINAL`/`BTYPE`, `LEN` and
//!   `NLEN` (one's complement of `LEN`), and followed by a big-endian
//!   Adler-32 checksum of the uncompressed data.
//! - **IEND**: empty terminator chunk.
//!
//! Every chunk carries a big-endian CRC-32 (IEEE 802.3 polynomial
//! `0xEDB8_8320`) over its type and payload.
//!
//! The trade-off is size: an *uncompressed* 800×600 RGBA image occupies about
//! 1.9 MB instead of the usually much smaller deflate-compressed file. That is
//! acceptable for Phase 4 (PNG output for tests and golden images); actual
//! compression is tracked as future work in WP-4.11. See
//! `docs/ARCHITECTURE.md` §3.1 for the dependency policy that motivates this
//! implementation.
//!
//! Typical use is through [`Pixmap::to_png`](crate::Pixmap::to_png), which
//! feeds the raw RGBA bytes to [`encode_rgba`].

/// The 8-byte PNG magic signature (PNG spec §5.2):
/// `\x89PNG\r\n\x1a\n`.
pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// CRC-32 lookup table for the IEEE polynomial, computed at compile time.
const CRC_TABLE: [u32; 256] = build_crc_table();

/// Builds the CRC-32 (IEEE 802.3) lookup table in a `const` context.
///
/// Rust `const fn` supports `while` loops (but not `for`), hence the manual
/// counters.
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

/// Feeds one byte into a running CRC-32 register (pre/post inversion handled
/// by the caller).
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
    // Largest n such that 255 * n * (n + 1) / 2 + (n + 1) * (MOD - 1) fits in
    // a u32; deferring the modulo keeps the inner loop branch-free.
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
    let length = data.len() as u32;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(kind);

    let mut crc = crc32_update(0xFFFF_FFFF, kind);
    crc = crc32_update(crc, data);
    crc ^= 0xFFFF_FFFF;

    out.extend_from_slice(data);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Wraps `data` in a zlib stream whose DEFLATE body uses stored blocks only.
///
/// The emitted bytes are a valid zlib stream (RFC 1950): 2-byte header with
/// `CM=8`, `CINFO=7`, `FCHECK` chosen so the header is a multiple of 31, the
/// stored DEFLATE blocks, and the big-endian Adler-32 of the uncompressed
/// input.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    // 0x78 = CMF (deflate, 32 KiB window), 0x01 = FLG such that
    // (0x78 << 8 | 0x01) % 31 == 0, no preset dictionary, no compression hint.
    let mut out = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);

    if data.is_empty() {
        // A final empty stored block is required to terminate the stream.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let total = data.len();
        for (index, block) in data.chunks(65_535).enumerate() {
            let is_final = (index + 1) * 65_535 >= total;
            let len = block.len() as u16;
            let nlen = !len;
            // BFINAL in bit 0, BTYPE=00 (stored) in bits 1..3 ⇒ 0x00 or 0x01.
            out.push(u8::from(is_final));
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(block);
        }
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Prefixes each RGBA scanline with the PNG filter type `0` (`None`).
fn build_scanlines(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity(height as usize * (stride + 1));
    for row in rgba.chunks(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    raw
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

    let raw = build_scanlines(width, height, rgba);
    let mut out = Vec::with_capacity(raw.len() + 128);
    out.extend_from_slice(&PNG_SIGNATURE);

    // IHDR: width, height, bit depth 8, color type 6 (RGBA), compression 0,
    // filter 0, interlace 0.
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8;
    ihdr[9] = 6;
    write_chunk(&mut out, b"IHDR", &ihdr);

    write_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut out, b"IEND", &[]);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

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

        (chunks, inflate_stored(&idat))
    }

    /// Minimal zlib decompressor that understands the stored-block subset
    /// emitted by this module.
    fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
        assert!(zlib.len() >= 6, "zlib stream too short");
        assert_eq!(zlib[0] & 0x0F, 8, "not a deflate stream");
        let mut pos = 2;
        let mut out = Vec::new();
        loop {
            assert!(pos < zlib.len(), "missing stored block header");
            let header = zlib[pos];
            assert_eq!(header & 0b110, 0, "expected stored DEFLATE block");
            let is_final = header & 1 == 1;
            pos += 1;

            let len = u16::from_le_bytes(zlib[pos..pos + 2].try_into().unwrap());
            let nlen = u16::from_le_bytes(zlib[pos + 2..pos + 4].try_into().unwrap());
            assert_eq!(len, !nlen, "LEN/NLEN mismatch");
            pos += 4;
            out.extend_from_slice(&zlib[pos..pos + len as usize]);
            pos += len as usize;
            if is_final {
                break;
            }
        }
        let stored_adler = u32::from_be_bytes(zlib[pos..pos + 4].try_into().unwrap());
        assert_eq!(stored_adler, adler32(&out), "adler-32 mismatch");
        out
    }

    fn rgba_of(scanlines: &[u8], width: u32, height: u32) -> Vec<u8> {
        let stride = width as usize * 4;
        let mut rgba = Vec::with_capacity(stride * height as usize);
        for row in scanlines.chunks(stride + 1) {
            assert_eq!(row[0], 0, "unexpected filter type");
            rgba.extend_from_slice(&row[1..]);
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
        assert_eq!(rgba_of(&inflated, 2, 2), rgba);
    }

    #[test]
    fn test_png_multiblock_stored_deflate() {
        // > 65 535 bytes of image data forces more than one stored block.
        let width = 200u32;
        let height = 100u32;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for i in 0..(width * height) {
            rgba.extend_from_slice(&[
                (i % 256) as u8,
                ((i / 3) % 256) as u8,
                ((i / 7) % 256) as u8,
                255,
            ]);
        }
        let png = encode_rgba(width, height, &rgba);
        let (_, inflated) = parse_png(&png);
        assert_eq!(inflated.len(), height as usize * (1 + width as usize * 4));
        assert_eq!(rgba_of(&inflated, width, height), rgba);
    }

    #[test]
    fn test_png_pixel_roundtrip_single_pixel() {
        let rgba = [12u8, 34, 56, 78];
        let png = encode_rgba(1, 1, &rgba);
        let (_, inflated) = parse_png(&png);
        assert_eq!(rgba_of(&inflated, 1, 1), rgba);
    }

    #[test]
    fn test_encode_rgba_rejects_inconsistent_buffers() {
        assert!(encode_rgba(0, 0, &[]).is_empty());
        assert!(encode_rgba(2, 2, &[0u8; 15]).is_empty());
    }
}
