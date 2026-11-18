use super::*;

#[test]
fn decodes_large_lossy_webp_and_scaled_path() {
    let bytes = fixture("photo_1024x768_lossy.webp");
    let image = decode(&bytes).expect("1024x768 VP8 keyframe");
    assert_eq!(image.dimensions(), (1024, 768));
    let oracle = fixture("photo_1024x768_lossy.rgba");
    assert_eq!(oracle.len(), 1024 * 768 * 4);
    let (max, mean) = compare_rgba(&image.data, &oracle);
    assert!(max <= 12, "1024x768: max channel error {max}");
    assert!(mean < 1.0, "1024x768: mean channel error {mean:.3}");

    // decode_scaled uses nearest row/column subsampling after the full decode:
    // output (ox, oy) comes from source (ox * 4, oy * 4) for a 1024px -> 256px scale.
    let scaled = megi_image::decode_scaled(&bytes, 256).expect("scaled 1024x768");
    assert_eq!(scaled.dimensions(), (256, 192));
    let mut max = 0i32;
    for oy in 0..192usize {
        for ox in 0..256usize {
            let source = ((oy * 4) * 1024 + ox * 4) * 4;
            let pixel = &scaled.data[(oy * 256 + ox) * 4..(oy * 256 + ox) * 4 + 4];
            for channel in 0..3 {
                let delta = (i32::from(pixel[channel]) - i32::from(oracle[source + channel])).abs();
                max = max.max(delta);
            }
        }
    }
    assert!(max <= 12, "scaled 256px: max channel error {max}");

    // Deterministic: decoding the same bytes twice yields identical pixels.
    let again = decode(&bytes).expect("second decode");
    assert_eq!(
        image.data, again.data,
        "VP8 lossy decode must be deterministic"
    );
}

#[test]
fn decodes_small_lossy_solid_fixture() {
    // 16x16 solid RGB(30, 160, 90) encoded by PIL at quality 80: lossy, so the
    // reconstructed pixels are close to (but not exactly) the source colour.
    let image = decode(&fixture("lossy.webp")).expect("16x16 VP8 keyframe");
    assert_eq!(image.dimensions(), (16, 16));
    for pixel in image.data.as_chunks::<4>().0 {
        for (channel, expected) in [(0i32, 30i32), (1, 160), (2, 90)] {
            let delta = (i32::from(pixel[channel as usize]) - expected).abs();
            assert!(delta <= 16, "channel {channel}: {pixel:?} vs {expected}");
        }
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn parses_real_pil_lossy_webp_keyframe_headers() {
    // The public header parser stays useful for tooling: dimensions/version/partition
    // size are validated before decoding, and `decode` now returns the pixels.
    for (name, width, height) in [
        ("photo_lossy_q75", 64u32, 48u32),
        ("photo_lossy_q90", 64, 48),
        ("photo_1024x768_lossy", 1024, 768),
    ] {
        let bytes = fixture(&format!("{name}.webp"));
        let info = megi_image::vp8::lossy_info(&bytes, &megi_image::PngLimits::default())
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!((info.width, info.height), (width, height), "{name}");
        assert!(info.version <= 3, "{name}");
        assert!(info.first_partition_size > 0, "{name}");
        let decoded = decode(&bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(decoded.dimensions(), (width, height), "{name}");
    }
}

#[test]
fn lossy_vp8_dimension_caps_are_enforced() {
    let limits = megi_image::PngLimits {
        max_dimension: 32,
        ..megi_image::PngLimits::default()
    };
    let error = megi_image::vp8::lossy_info(&fixture("photo_lossy_q75.webp"), &limits)
        .expect_err("dimension cap");
    assert!(
        matches!(error, ImageError::LimitExceeded { .. }),
        "{error:?}"
    );
    // The pixel decoder enforces the same caps (before allocating the RGBA buffer).
    let error = megi_image::decode_with_limits(&fixture("photo_lossy_q75.webp"), &limits)
        .expect_err("decode dimension cap");
    assert!(
        matches!(error, ImageError::LimitExceeded { .. }),
        "{error:?}"
    );
}

#[test]
fn corrupt_lossy_webp_is_a_typed_error() {
    let bytes = fixture("photo_lossy_q75.webp");
    // Container/header prefixes are rejected by the header parser.
    for length in [0, 4, 12, 20] {
        let error = megi_image::vp8::lossy_info(
            &bytes[..length.min(bytes.len())],
            &megi_image::PngLimits::default(),
        )
        .expect_err("truncated lossy");
        assert!(
            matches!(
                error,
                ImageError::Truncated { .. }
                    | ImageError::InvalidData { .. }
                    | ImageError::InvalidSignature
            ),
            "length {length}: {error:?}"
        );
    }
    // Truncated VP8 payloads (container intact, partitions cut) must also fail with a
    // typed error from `decode`, never panic or return random pixels.
    for fraction in [2usize, 4, 8, 16] {
        let length = bytes.len() / fraction;
        if length == 0 || length >= bytes.len() {
            continue;
        }
        let error = decode(&bytes[..length]).expect_err("truncated VP8 payload");
        assert!(
            matches!(
                error,
                ImageError::Truncated { .. }
                    | ImageError::InvalidData { .. }
                    | ImageError::InvalidSignature
                    | ImageError::LimitExceeded { .. }
            ),
            "length {length}: {error:?}"
        );
    }
}

#[test]
fn decodes_1024x768_lossy_within_time_budget() {
    // A/B note: this replaces the header-only timing check of the previous phase. The
    // full 1024x768 VP8 keyframe decode (3072 macroblocks) runs in a few tens of ms in
    // release; the bound is deliberately loose for debug/CI machines.
    use std::time::Instant;
    let bytes = fixture("photo_1024x768_lossy.webp");
    let start = Instant::now();
    let image = decode(&bytes).expect("1024x768 lossy");
    let elapsed = start.elapsed();
    assert_eq!(image.dimensions(), (1024, 768));
    eprintln!(
        "VP8 lossy 1024x768 decode: {:.2} ms",
        elapsed.as_secs_f64() * 1e3
    );
    assert!(elapsed.as_secs() < 5, "decode too slow: {elapsed:?}");
}
