#![no_main]
//! Fuzz target: WOFF2 web-font decoding.
//!
//! `@font-face src: url(...)` bytes are untrusted and reach a third-party
//! WOFF2→sfnt converter whose header/directory parsing is the risky part. The target
//! checks that malformed input always returns a typed [`fonts::FontError`] (no panic
//! escaping, no unbounded allocation) and that decoded sfnt stays under the documented cap.

use libfuzzer_sys::fuzz_target;

/// Mirrors the module's `MAX_SFNT_BYTES` (the decoder must never return more).
const MAX_SFNT_BYTES: usize = 64 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    // Web fonts are fetched under an 8 MiB cap; keep iterations cheap.
    if data.len() > 256 * 1024 {
        return;
    }
    if let Ok(sfnt) = fonts::decode_web_font(data) {
        assert!(
            sfnt.len() <= MAX_SFNT_BYTES,
            "decoded sfnt exceeds the cap: {} bytes",
            sfnt.len()
        );
    }
});
