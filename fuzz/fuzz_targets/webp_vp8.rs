#![no_main]
//! Fuzz target: WebP sniffing + decoding, focused on the VP8 lossy path.
//!
//! Lossy `VP8 ` is currently detected and rejected with a typed
//! `ImageError::UnsupportedFeature` by the image crate (the VP8L lossless decoder
//! is implemented); this target keeps the sniff/container/validation surface
//! fuzzed so the lossy decoder can land behind the same target. The input
//! (≤512 KiB) is:
//!
//! 1. fed directly to [`image::detect_format`] and
//!    [`image::decode_with_limits`] with tight anti-DoS limits;
//! 2. when it is not already a RIFF/WEBP container, wrapped twice in a minimal
//!    RIFF container — once with a `VP8 ` chunk (lossy) and once with `VP8L`
//!    (lossless) — to exercise chunk-size/RIFF-size/truncation validation;
//! 3. decoded only through [`image::decode_with_limits`]: the defensive limits
//!    (1024 px/side, 512 Ki pixels) make even a hostile synthesized dimension
//!    header cheap, unlike the default limits.
//!
//! Ok/Err results are intentionally discarded: panics, aborts, ASan/UBSan
//! findings, integer overflows and timeouts are the signals.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer input (decoders have stricter internal caps).
const MAX_FUZZ_INPUT: usize = 512 * 1024;
/// Cap on bytes wrapped into a synthesized RIFF chunk.
const MAX_CHUNK: usize = 8192;

fn riff_webp(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + payload.len());
    out.extend_from_slice(b"RIFF");
    // RIFF size covers "WEBP" + chunk header + (padded) payload, little-endian.
    let padded = payload.len() + (payload.len() & 1);
    let riff_size = 4u32 + 8 + padded as u32;
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(chunk);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() & 1 == 1 {
        out.push(0);
    }
    out
}

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
    // VP8 lossy chunk: exercises sniffing, RIFF/chunk size validation and the
    // typed UnsupportedFeature path until the lossy decoder lands.
    let lossy = riff_webp(b"VP8 ", payload);
    let _ = image::decode_with_limits(&lossy, &limits);
    // VP8L lossless chunk: malformed-but-sized payloads keep the full decoder
    // validation path covered from the same corpus.
    let lossless = riff_webp(b"VP8L", payload);
    let _ = image::decode_with_limits(&lossless, &limits);
    // Truncated container variants (early loop holes in size arithmetic).
    let _ = image::decode_with_limits(&lossy[..lossy.len() / 2], &limits);
    let _ = image::decode_with_limits(&lossless[..lossless.len() / 2], &limits);
});
