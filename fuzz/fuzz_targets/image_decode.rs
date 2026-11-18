#![no_main]
//! Fuzz target: image sniffing + every public decode entry point (PNG/JPEG/GIF/WebP).
//!
//! Earlier revisions only drove [`megi_image::decode_with_limits`] with one tight
//! [`megi_image::PngLimits`] profile, so the *default* caps, the scaled-decode path and the
//! animated (WebP ANIM/ANMF) path were never fuzzed (#853). This target now covers:
//!
//! - [`megi_image::detect_format`] (sniffing must never panic);
//! - [`megi_image::decode_with_limits`] with **two** limit profiles derived from the input
//!   (a tight one and a roomier one), so cap arithmetic is exercised on both sides;
//! - [`megi_image::decode`] with the crate defaults;
//! - [`megi_image::decode_scaled`] / [`megi_image::decode_scaled_with_limits`] with a max dimension
//!   taken from the input, including the 0/1/huge edge cases;
//! - [`megi_image::decode_animated`] / [`megi_image::decode_animated_with_limits`] for containers
//!   with several frames;
//! - [`megi_image::png::decode_png`] directly, bypassing the sniffer;
//! - [`megi_image::exif::orientation`] on every input.
//!
//! Ok/Err results are discarded: the target looks for panics, aborts, ASan/UBSan findings,
//! integer overflows and timeouts.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided file size (decoder caps are stricter).
const MAX_FUZZ_INPUT: usize = 512 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let _ = megi_image::detect_format(data);

    // Tight profile: a malicious header cannot allocate more than a few MiB.
    let tight = megi_image::PngLimits::new(1024, 512 * 1024, 128 * 1024, 4 * 1024 * 1024);
    // Roomier profile (still bounded): reaches the dimension/pixel arithmetic that the
    // tight caps reject early. Derived from the input so both profiles get coverage.
    let max_dim = 256u32 + u32::from(data.first().copied().unwrap_or(0) % 8) * 512;
    let roomy = megi_image::PngLimits::new(
        max_dim,
        u64::from(max_dim) * u64::from(max_dim),
        512 * 1024,
        8 * 1024 * 1024,
    );

    let _ = megi_image::decode_with_limits(data, &tight);
    let _ = megi_image::decode_with_limits(data, &roomy);
    let _ = megi_image::decode(data);

    // Scaled decode: the max dimension comes from the input, including degenerate values.
    let scaled_dim = megi_fuzz::read_u32(data, data.len().saturating_sub(4)) % 2048;
    let _ = megi_image::decode_scaled(data, scaled_dim);
    let _ = megi_image::decode_scaled_with_limits(data, &tight, scaled_dim);

    // Animated containers (WebP ANIM/ANMF, and whatever else grows frame support).
    let _ = megi_image::decode_animated(data);
    let _ = megi_image::decode_animated_with_limits(data, &tight);

    // Direct PNG entry point: no sniffer in front of the decoder.
    let _ = megi_image::png::decode_png(data, &tight);

    let _ = megi_image::exif::orientation(data);
});
