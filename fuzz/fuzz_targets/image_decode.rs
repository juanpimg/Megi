#![no_main]
//! Fuzz target: image sniffing + decoding (PNG/JPEG/GIF; WebP detected-not-decoded)
//! plus the EXIF orientation parser, with strict input and decoder caps.
//!
//! The input (≤512 KiB) is fed to [`image::detect_format`] and
//! [`image::decode_with_limits`] with reduced limits (1024 px per side, 512 Ki
//! pixels, 128 KiB per chunk, 4 MiB of IDAT): a malicious header/stream cannot
//! allocate more than a few MiB or run unbounded decompression. WebP bytes are
//! detected and rejected as `UnsupportedFormat` by design; JPEG progressive/CMYK
//! paths return typed errors. The EXIF APP1 parser is also exercised on every
//! input. Ok/Err results are intentionally discarded: the target looks for
//! panics, aborts, ASan/UBSan findings, integer overflows and timeouts.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided file size (decoder caps are stricter).
const MAX_FUZZ_INPUT: usize = 512 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let _ = image::detect_format(data);
    let limits = image::PngLimits::new(
        1024,            // max dimension
        512 * 1024,      // max pixels
        128 * 1024,      // max chunk bytes
        4 * 1024 * 1024, // max total IDAT bytes
    );
    let _ = image::decode_with_limits(data, &limits);
    let _ = image::exif::orientation(data);
});
