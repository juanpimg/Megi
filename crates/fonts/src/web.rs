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
//! - The decoder validates the WOFF2 container and reconstructed `glyf`/`loca`/`hmtx`
//!   tables; malformed input returns [`FontError::InvalidWebFont`] and never panics.
//! - No subsetting, color-font (`COLR`/`CBDT`) or variation-specific handling beyond
//!   what the WOFF2 container itself requires.

use crate::error::FontError;

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
    let mut cursor = std::io::Cursor::new(bytes);
    woff2::convert_woff2_to_ttf(&mut cursor).map_err(|error| FontError::InvalidWebFont {
        reason: error.to_string(),
    })
}
