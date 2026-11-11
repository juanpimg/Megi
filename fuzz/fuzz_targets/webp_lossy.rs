#![no_main]
//! Fuzz target: WebP **VP8 lossy keyframe** decoding (new decoder, R228).
//!
//! The image crate now decodes `VP8 ` keyframes (with anti-DoS caps). This target feeds:
//!
//! 1. the input directly to [`image::detect_format`] / [`image::decode_with_limits`]
//!    (arbitrary bytes, so a non-WebP signature is rejected by sniffing);
//! 2. every RIFF/WEBP input as-is and with truncated prefixes (container/VP8 header
//!    validation);
//! 3. non-RIFF payloads wrapped in a minimal `RIFF....WEBPVP8 ` container, twice: raw and
//!    truncated, so the VP8 keyframe parser/decoder is reached even from non-container
//!    seeds.
//!
//! Caps: ≤512 KiB input, 2048 px/side, 2 Mi pixels, 128 KiB chunks — an adversarial
//! dimension header cannot make one iteration expensive. Panics, aborts, ASan/UBSan
//! findings, overflows and timeouts are the signals; results are discarded.

use libfuzzer_sys::fuzz_target;

const MAX_FUZZ_INPUT: usize = 512 * 1024;
const MAX_CHUNK: usize = 128 * 1024;

fn riff_vp8(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + payload.len());
    out.extend_from_slice(b"RIFF");
    let padded = payload.len() + (payload.len() & 1);
    out.extend_from_slice(&(4u32 + 8 + padded as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(b"VP8 ");
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
        2048,            // max dimension
        2 * 1024 * 1024, // max pixels
        128 * 1024,      // max chunk bytes
        4 * 1024 * 1024, // max total IDAT bytes
    );

    // Sniffing must classify the bytes without panicking.
    let format = image::detect_format(data);
    let _ = format;

    // Valid/truncated WebP inputs go straight to the decoder.
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        let _ = image::decode_with_limits(data, &limits);
        for divisor in [2, 4, 8] {
            let _ = image::decode_with_limits(&data[..data.len() / divisor], &limits);
        }
        // A VP8 chunk with a corrupted payload still exercises header validation.
        let payload = &data[20.min(data.len())..];
        let _ = image::decode_with_limits(&riff_vp8(payload), &limits);
        return;
    }

    // Non-container seeds: wrap bounded payloads as a lossy keyframe.
    let payload = &data[..data.len().min(MAX_CHUNK)];
    let wrapped = riff_vp8(payload);
    let _ = image::decode_with_limits(&wrapped, &limits);
    let _ = image::decode_with_limits(&wrapped[..wrapped.len() / 2], &limits);
});
