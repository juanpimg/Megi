#![no_main]
//! Fuzz target: WebP decoding, sniffing and RIFF/VP8/VP8L container validation.
//!
//! The VP8L lossless, VP8 lossy keyframe, VP8X/ALPH and animated ANIM/ANMF decoders all
//! sit behind the same RIFF/chunk validation layer. This single target keeps the payload
//! grammars **and** the envelope fuzzed together (the two former targets, `webp_vp8` and
//! `webp_lossy`, shared the `megi_fuzz::riff_webp` helper and overlapped heavily (#471)):
//!
//! 1. the input goes to [`megi_image::detect_format`] and to [`megi_image::decode_with_limits`]
//!    under three caps (a wide profile, a tight profile, and the smallest legal dimension)
//!    so hostile dimension headers are rejected before any allocation;
//! 2. RIFF/WEBP inputs are decoded as-is, truncated at several divisors, and their VP8
//!    payload is re-wrapped in a fresh container to exercise header validation;
//! 3. non-container payloads are wrapped as both `VP8 ` (lossy keyframe) and `VP8L`
//!    (lossless) chunks, also truncated, and a bounded variant is decoded with the
//!    **default** limits because the tight caps alone never reach the dimension
//!    arithmetic of a plausible image (#853).
//!
//! Ok/Err results are intentionally discarded: panics, aborts, ASan/UBSan findings,
//! integer overflows and timeouts are the signals.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input (decoders have stricter internal caps).
const MAX_FUZZ_INPUT: usize = 512 * 1024;
/// Cap on bytes wrapped into a synthesized RIFF chunk.
const MAX_CHUNK: usize = 128 * 1024;

fn caps(dimension: u32) -> megi_image::PngLimits {
    megi_image::PngLimits::new(
        dimension,
        u64::from(dimension) * u64::from(dimension).max(1),
        128 * 1024,
        4 * 1024 * 1024,
    )
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let wide = caps(2048);
    let tight = caps(64);

    let _ = megi_image::detect_format(data);
    let _ = megi_image::decode_with_limits(data, &wide);
    let _ = megi_image::decode_with_limits(data, &tight);

    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        for divisor in [2, 4, 8] {
            let _ = megi_image::decode_with_limits(&data[..data.len() / divisor], &wide);
        }
        // A VP8 chunk with a corrupted payload still exercises header validation.
        let payload = &data[20.min(data.len())..][..data.len().saturating_sub(20).min(MAX_CHUNK)];
        let _ = megi_image::decode_with_limits(&megi_fuzz::riff_webp(b"VP8 ", payload), &wide);
        return;
    }

    let payload = &data[..data.len().min(MAX_CHUNK)];
    let lossy = megi_fuzz::riff_webp(b"VP8 ", payload);
    let _ = megi_image::decode_with_limits(&lossy, &wide);
    let _ = megi_image::decode_with_limits(&lossy, &tight);
    let _ = megi_image::decode_with_limits(&lossy[..lossy.len() / 2], &wide);

    let lossless = megi_fuzz::riff_webp(b"VP8L", payload);
    let _ = megi_image::decode_with_limits(&lossless, &wide);
    let _ = megi_image::decode_with_limits(&lossless[..lossless.len() / 2], &wide);

    // Default limits: the caps above can hide the size arithmetic of a plausible image.
    let _ = megi_image::decode(&lossy[..lossy.len().min(64 * 1024)]);
});
