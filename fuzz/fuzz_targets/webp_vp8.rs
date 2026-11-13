#![no_main]
//! Fuzz target: WebP sniffing + empty/container decoding across the RIFF envelope.
//!
//! The lossy VP8 keyframe and the lossless VP8L decoder (plus VP8X/ALPH and animated
//! ANIM/ANMF containers) all sit behind the same RIFF/chunk validation layer. This target
//! keeps that envelope fuzzed independently of the VP8 payload grammar:
//!
//! 1. the input is fed to [`image::detect_format`] and
//!    [`image::decode_with_limits`] with defensive limits;
//! 2. when it is not already a RIFF/WEBP container, it is wrapped twice with
//!    [`megi_fuzz::riff_webp`] — once as `VP8 ` (lossy) and once as `VP8L` (lossless) — to
//!    exercise chunk-size/RIFF-size/padding/truncation validation;
//! 3. the wrapped variants are also decoded with the **default** (`image::decode`) limits,
//!    because the tight-cap pass alone never reaches the dimension arithmetic of a
//!    legitimate-size image (#853).
//!
//! Ok/Err results are intentionally discarded: panics, aborts, ASan/UBSan findings, integer
//! overflows and timeouts are the signals.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input (decoders have stricter internal caps).
const MAX_FUZZ_INPUT: usize = 512 * 1024;
/// Cap on bytes wrapped into a synthesized RIFF chunk.
const MAX_CHUNK: usize = 8192;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let limits = image::PngLimits::new(
        1024,            // max dimension
        512 * 1024,      // max pixels
        128 * 1024,      // max chunk bytes
        4 * 1024 * 1024, // max total IDAT bytes
    );

    let _ = image::detect_format(data);
    let _ = image::decode_with_limits(data, &limits);

    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        return;
    }

    let payload = &data[..data.len().min(MAX_CHUNK)];
    // VP8 lossy chunk: exercises sniffing, RIFF/chunk size validation and the VP8
    // keyframe decoder.
    let lossy = megi_fuzz::riff_webp(b"VP8 ", payload);
    let _ = image::decode_with_limits(&lossy, &limits);
    // VP8L lossless chunk: malformed-but-sized payloads keep the full decoder validation
    // path covered from the same corpus.
    let lossless = megi_fuzz::riff_webp(b"VP8L", payload);
    let _ = image::decode_with_limits(&lossless, &limits);
    // Truncated container variants (early loop holes in size arithmetic).
    let _ = image::decode_with_limits(&lossy[..lossy.len() / 2], &limits);
    let _ = image::decode_with_limits(&lossless[..lossless.len() / 2], &limits);
    // Default limits: the caps above can hide the size arithmetic of a plausible image.
    let _ = image::decode(&lossy[..lossy.len().min(64 * 1024)]);
});
