//! Web-font decoding (WOFF2 → TrueType/OpenType bytes).
//!
//! `@font-face` sources are commonly served as WOFF2, which is a **compressed
//! container**: `fontdb`/`ttf-parser` only understand raw sfnt (TTF/OTF). This module
//! converts WOFF2 into sfnt bytes before registration in [`crate::FontDatabase`].
//!
//! # Dependency policy
//!
//! Decoding uses the pure-Rust fork [`woff2-patched`](https://crates.io/crates/woff2-patched)
//! **0.4** (imported as `woff2`, Apache-2.0, no C/FFI, no system libraries), authorized by the owner for Phase 6 web-font support. Keeping it
//! in `fonts` (rather than `render`) means the HTML/render layers never see compressed
//! font data.
//!
//! # Limits
//!
//! - WOFF1 is **not** decoded (only WOFF2; TTF/OTF pass through unchanged).
//! - The header and the whole table directory are validated before handing the bytes over
//!   (`totalSfntSize`, `totalCompressedSize`, `numTables`, file length and every declared
//!   table/transform length), the reconstructed sfnt is capped at [`MAX_SFNT_BYTES`], and
//!   the third-party conversion runs under `catch_unwind`: malformed input returns
//!   [`FontError::InvalidWebFont`] and never panics.
//! - Residual: the dependency decompresses Brotli into an unbounded `Vec`, so a stream that
//!   inflates past the validated lengths is not stopped before allocation. Bounding it
//!   requires patching or vendoring the decoder.
//! - No subsetting, color-font (`COLR`/`CBDT`) or variation-specific handling beyond
//!   what the WOFF2 container itself requires.

use crate::error::FontError;

/// Maximum reconstructed sfnt size accepted from one WOFF2 container.
pub const MAX_SFNT_BYTES: usize = 64 * 1024 * 1024;

/// Fixed WOFF2 header length (W3C WOFF2 §4.1).
const WOFF2_HEADER_LEN: usize = 48;

fn be_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn be_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Reads a WOFF2 `UIntBase128` (1-5 bytes, no leading zero padding).
fn read_base128(bytes: &[u8], position: &mut usize) -> Option<u32> {
    let mut value: u32 = 0;
    for index in 0..5 {
        let byte = *bytes.get(*position)?;
        *position += 1;
        if index == 0 && byte == 0x80 {
            return None;
        }
        value = value
            .checked_mul(128)?
            .checked_add(u32::from(byte & 0x7F))?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Validates the WOFF2 header and table directory before the decoder allocates.
///
/// `woff2-patched` 0.4 trusts `totalSfntSize`, `totalCompressedSize` and the table
/// directory: a crafted header requested a 4 GiB allocation, a single directory
/// `origLength` of 4,000,000,000 reached `Vec::with_capacity(Σ src_length)` despite a
/// 64 KiB `totalSfntSize`, and a directory length one byte past the compressed stream
/// panicked on a slice range. Parse the directory and bound every declared length here.
fn validate_woff2_header(bytes: &[u8]) -> Result<(), FontError> {
    let invalid = |reason: &str| FontError::InvalidWebFont {
        reason: reason.to_string(),
    };
    if bytes.len() < WOFF2_HEADER_LEN {
        return Err(invalid("truncated WOFF2 header"));
    }
    if &bytes[..4] != b"wOF2" {
        return Err(invalid("bad WOFF2 signature"));
    }
    // Collections (`ttcf`) carry an extra collection directory after the table directory,
    // so the compressed-stream bound below would be wrong for them; they are not a web-font
    // format served through `@font-face`, so reject them instead of validating partially.
    if &bytes[4..8] == b"ttcf" {
        return Err(invalid("WOFF2 font collections are not supported"));
    }
    if be_u32(bytes, 8) as usize > bytes.len() {
        return Err(invalid("WOFF2 length field exceeds the input"));
    }
    let num_tables = be_u16(bytes, 12);
    if num_tables == 0 || num_tables > 0x3FFF {
        return Err(invalid("WOFF2 numTables out of range"));
    }
    let total_sfnt = be_u32(bytes, 16) as usize;
    if total_sfnt == 0 || total_sfnt > MAX_SFNT_BYTES {
        return Err(invalid("WOFF2 totalSfntSize exceeds the cap"));
    }
    let total_compressed = be_u32(bytes, 20) as usize;
    if total_compressed == 0 || total_compressed > bytes.len() {
        return Err(invalid("WOFF2 totalCompressedSize exceeds the input"));
    }
    // Table directory: sum the declared lengths (the decoder reserves Σ src_length and
    // grows the output by orig_length), so a per-entry value cannot bypass the header caps.
    let mut position = WOFF2_HEADER_LEN;
    let mut source_total = 0usize;
    let mut original_total = 0usize;
    for _ in 0..num_tables {
        let flags = *bytes
            .get(position)
            .ok_or_else(|| invalid("truncated WOFF2 table directory"))?;
        position += 1;
        let table_ref = flags & 0x3F;
        let version = flags & 0xC0;
        let tag: [u8; 4] = if table_ref == 0x3F {
            let tag = bytes
                .get(position..position + 4)
                .ok_or_else(|| invalid("truncated WOFF2 table tag"))?;
            position += 4;
            [tag[0], tag[1], tag[2], tag[3]]
        } else {
            match table_ref {
                10 => *b"glyf",
                11 => *b"loca",
                _ => [0; 4],
            }
        };
        let orig_length = read_base128(bytes, &mut position)
            .ok_or_else(|| invalid("invalid WOFF2 table length"))?
            as usize;
        // Mirrors the dependency's transform rule: glyf/loca use version 0xC0 for the
        // null transform, every other table version 0x00.
        let null_transform = if &tag == b"glyf" || &tag == b"loca" {
            version == 0xC0
        } else {
            version == 0x00
        };
        let source_length = if null_transform {
            orig_length
        } else {
            read_base128(bytes, &mut position)
                .ok_or_else(|| invalid("invalid WOFF2 transform length"))? as usize
        };
        source_total = source_total.saturating_add(source_length);
        original_total = original_total.saturating_add(orig_length);
        if source_total > MAX_SFNT_BYTES || original_total > MAX_SFNT_BYTES {
            return Err(invalid("WOFF2 table directory lengths exceed the cap"));
        }
    }
    if position.saturating_add(total_compressed) > bytes.len() {
        return Err(invalid("WOFF2 compressed data exceeds the input"));
    }
    Ok(())
}

/// Returns whether `bytes` look like a WOFF2 container (magic `wOF2`).
pub fn is_woff2(bytes: &[u8]) -> bool {
    woff2::decode::is_woff2(bytes)
}

/// Decodes WOFF2 bytes into raw sfnt (TTF/OTF) bytes.
///
/// Raw TTF/OTF input is returned unchanged (copy), so callers can use this as a
/// "normalize web font data" step for mixed `src` lists.
pub fn decode_web_font(bytes: &[u8]) -> Result<Vec<u8>, FontError> {
    if bytes.is_empty() {
        return Err(FontError::InvalidWebFont {
            reason: "empty web font data".to_string(),
        });
    }
    if !is_woff2(bytes) {
        return Ok(bytes.to_vec());
    }
    validate_woff2_header(bytes)?;
    let mut cursor = std::io::Cursor::new(bytes);
    // The third-party decoder has unchecked slice ranges and trusts the header sizes (see
    // the module limits); a panic on hostile input must become a typed error rather than
    // unwind through the renderer. The header/directory validation bounds the declared
    // allocations; a Brotli stream that inflates past them is a documented residual.
    let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        woff2::convert_woff2_to_ttf(&mut cursor)
    }))
    .map_err(|_| FontError::InvalidWebFont {
        reason: "WOFF2 decoder panicked on malformed input".to_string(),
    })?;
    let sfnt = converted.map_err(|error| FontError::InvalidWebFont {
        reason: error.to_string(),
    })?;
    if sfnt.len() > MAX_SFNT_BYTES {
        return Err(FontError::InvalidWebFont {
            reason: format!("reconstructed font exceeds {MAX_SFNT_BYTES} bytes"),
        });
    }
    Ok(sfnt)
}
