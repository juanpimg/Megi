use super::*;

#[test]
fn detects_webp_container() {
    assert_eq!(
        detect_format(&fixture("v8l_two_color.webp")),
        ImageFormat::WebP
    );
}

#[test]
fn decodes_vp8l_two_colour_fixture_exactly() {
    let image = decode(&fixture("v8l_two_color.webp")).expect("VP8L");
    assert_eq!(image.dimensions(), (8, 8));
    assert_eq!(image.data, fixture("v8l_two_color.rgba"));
    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(7, 7), Some([0, 0, 255, 255]));
}

#[test]
fn decodes_vp8l_alpha_fixture_exactly() {
    let image = decode(&fixture("v8l_alpha.webp")).expect("VP8L alpha");
    assert_eq!(image.dimensions(), (8, 8));
    assert_eq!(image.data, fixture("v8l_alpha.rgba"));
    assert_eq!(image.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(image.pixel(7, 7), Some([0, 0, 255, 0]), "transparent half");
}

#[test]
fn decodes_vp8l_colour_indexing_fixture_exactly() {
    // PIL/libwebp encodes a solid colour with the colour-indexing transform.
    let image = decode(&fixture("palette_transform.webp")).expect("palette transform");
    assert_eq!(image.dimensions(), (16, 16));
    assert_eq!(image.data, fixture("palette_transform.rgba"));
}

#[test]
fn decodes_real_pil_lossless_photos_exactly() {
    // Real libwebp lossless encodings: predictor/colour transforms, palette and alpha.
    for (name, width, height) in [
        ("photo_transforms", 64u32, 48u32),
        ("palette_real", 32, 24),
        ("alpha_real", 40, 32),
    ] {
        let image = decode(&fixture(&format!("{name}.webp")))
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(image.dimensions(), (width, height), "{name}");
        let expected = fixture(&format!("{name}.rgba"));
        assert_eq!(image.data.len(), expected.len(), "{name}");
        assert_eq!(
            image.data, expected,
            "{name}: pixels must match PIL exactly"
        );
    }
}

#[test]
fn decodes_real_pil_lossy_webp_within_tolerance() {
    // PIL/libwebp q75 and q90 encodings of the same 64x48 synthetic photo. The decoder
    // is a from-scratch VP8 keyframe port: pixels are compared with the PIL RGBA oracle
    // under a documented tolerance (per-channel max and mean absolute error).
    for (name, max_tolerance) in [("photo_lossy_q75", 12), ("photo_lossy_q90", 8)] {
        let image = decode(&fixture(&format!("{name}.webp")))
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(image.dimensions(), (64, 48), "{name}");
        let oracle = fixture(&format!("{name}.rgba"));
        assert_eq!(image.data.len(), oracle.len(), "{name}");
        let (max, mean) = compare_rgba(&image.data, &oracle);
        assert!(
            max <= max_tolerance,
            "{name}: max channel error {max} exceeds {max_tolerance}"
        );
        assert!(
            mean < 1.0,
            "{name}: mean channel error {mean:.3} exceeds 1.0"
        );
        assert!(
            image
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255),
            "{name}: lossy WebP has no alpha, pixels must be opaque"
        );
    }
}
