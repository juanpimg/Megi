#![no_main]
//! Fuzz target: WebP **VP8 lossy keyframe** decoding.
//!
//! The image crate decodes `VP8 ` keyframes (with anti-DoS caps). This target feeds:
//!
//! 1. the input directly to [`image::detect_format`] / [`image::decode_with_limits`]
//!    (arbitrary bytes, so a non-WebP signature is rejected by sniffing);
//! 2. every RIFF/WEBP input as-is, with truncations and with the payload re-wrapped in a
//!    fresh `VP8 ` container (container/VP8 header validation);
//! 3. non-RIFF payloads wrapped in a minimal `RIFF....WEBPVP8 ` container, twice: raw and
//!    truncated, so the VP8 keyframe parser/decoder is reached even from non-container
//!    seeds.
//!
//! The RIFF envelope is built with [`megi_fuzz::riff_webp`], shared with the `webp_vp8`
//! target so the two harnesses cannot drift apart (#859).
//!
//! Caps: ≤512 KiB input, 2048 px/side, 2 Mi pixels, 128 KiB chunks — an adversarial
//! dimension header cannot make one iteration expensive. Panics, aborts, ASan/UBSan
//! findings, overflows and timeouts are the signals; results are discarded.

use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_INPUT: usize = 512 * 1024;
const MAX_CHUNK: usize = 128 * 1024;

fn caps(dimension: u32) -> image::PngLimits {
    image::PngLimits::new(
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
    let limits = caps(2048);
    // A second pass with the smallest legal dimension cap: hostile headers that claim a
    // huge canvas must be rejected before any allocation, both above and below the cap.
    let tight = caps(64);

    let _ = image::detect_format(data);

    // Valid/truncated WebP inputs go straight to the decoder.
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        let _ = image::decode_with_limits(data, &limits);
        let _ = image::decode_with_limits(data, &tight);
        for divisor in [2, 4, 8] {
            let _ = image::decode_with_limits(&data[..data.len() / divisor], &limits);
        }
        // A VP8 chunk with a corrupted payload still exercises header validation.
        let payload = &data[20.min(data.len())..][..data.len().saturating_sub(20).min(MAX_CHUNK)];
        let _ = image::decode_with_limits(&megi_fuzz::riff_webp(b"VP8 ", payload), &limits);
        return;
    }

    // Non-container seeds: wrap bounded payloads as a lossy keyframe.
    let payload = &data[..data.len().min(MAX_CHUNK)];
    let wrapped = megi_fuzz::riff_webp(b"VP8 ", payload);
    let _ = image::decode_with_limits(&wrapped, &limits);
    let _ = image::decode_with_limits(&wrapped[..wrapped.len() / 2], &limits);
    // The same bytes as a lossless chunk: the VP8L decoder must stay robust when the lossy
    // target is the one mutating it.
    let lossless = megi_fuzz::riff_webp(b"VP8L", payload);
    let _ = image::decode_with_limits(&lossless, &limits);
});
