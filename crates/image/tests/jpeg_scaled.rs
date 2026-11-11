//! JPEG reduced-IDCT scaled decoding and EXIF thumbnail APIs (R178).

use std::fs;
use std::path::PathBuf;

use image::{ImageError, RgbaImage, decode, decode_scaled, exif_thumbnail};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(path).expect("fixture")
}

fn rgba_fixture(name: &str) -> Vec<u8> {
    fixture(name)
}

fn diff_stats(actual: &[u8], expected: &[u8]) -> (i32, f64) {
    assert_eq!(actual.len(), expected.len());
    let mut max = 0i32;
    let mut sum = 0i64;
    for (a, b) in actual.iter().zip(expected.iter()) {
        let delta = (*a as i32 - *b as i32).abs();
        max = max.max(delta);
        sum += delta as i64;
    }
    (max, sum as f64 / expected.len() as f64)
}

/// Nearest resize with the same mapping as `image`'s internal helper (independent copy).
fn nearest_resize(image: &RgbaImage, target_width: u32, target_height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((target_width * target_height * 4) as usize);
    for y in 0..target_height {
        let source_y = ((y as u64 * image.height as u64) / target_height as u64) as u32;
        for x in 0..target_width {
            let source_x = ((x as u64 * image.width as u64) / target_width as u64) as u32;
            let offset = ((source_y * image.width + source_x) * 4) as usize;
            data.extend_from_slice(&image.data[offset..offset + 4]);
        }
    }
    data
}

#[test]
fn reduced_idct_dimensions_are_exact_for_scales_1_2_4_8() {
    let bytes = fixture("jpeg/reduced_1024x768.jpg");
    for (max_dim, expected) in [
        (1024u32, (1024u32, 768u32)),
        (512, (512, 384)),
        (256, (256, 192)),
        (128, (128, 96)),
    ] {
        let image = decode_scaled(&bytes, max_dim).expect("scaled JPEG");
        assert_eq!(image.dimensions(), expected, "max_dim={max_dim}");
    }
}

#[test]
fn reduced_idct_matches_pil_draft_within_documented_tolerance() {
    let bytes = fixture("jpeg/reduced_1024x768.jpg");
    for scale in [2u32, 4, 8] {
        let image = decode_scaled(&bytes, 1024 / scale).expect("scaled JPEG");
        let oracle = rgba_fixture(&format!("jpeg/reduced_1024x768_s{scale}.rgba"));
        let (max, mean) = diff_stats(&image.data, &oracle);
        // Documented tolerance vs libjpeg's scaled IDCT (PIL draft): per-channel ±4,
        // mean absolute error < 1 for smooth photos.
        assert!(max <= 4, "scale {scale}: max diff {max}");
        assert!(mean < 1.0, "scale {scale}: mean diff {mean}");
    }
}

#[test]
fn full_decode_is_unchanged_and_matches_pil() {
    let bytes = fixture("jpeg/reduced_1024x768.jpg");
    let image = decode(&bytes).expect("full decode");
    assert_eq!(image.dimensions(), (1024, 768));
    let oracle = rgba_fixture("jpeg/reduced_1024x768.rgba");
    let (max, mean) = diff_stats(&image.data, &oracle);
    assert!(max <= 4, "full decode max diff {max}");
    assert!(mean < 0.5, "full decode mean diff {mean}");
}

#[test]
fn exif_thumbnail_is_extracted_and_decoded() {
    let bytes = fixture("jpeg/exif_thumb.jpg");
    let thumbnail = exif_thumbnail(&bytes).expect("embedded EXIF thumbnail");
    assert_eq!(thumbnail.dimensions(), (160, 120));
    let oracle = rgba_fixture("jpeg/exif_thumb_thumbnail.rgba");
    // The embedded thumbnail is decoded by the in-tree JPEG decoder: documented
    // tolerance is per-channel ±4 typical; a handful of IDCT rounding outliers up to
    // ±6 are accepted for the 160x120 thumbnail.
    let (max, mean) = diff_stats(&thumbnail.data, &oracle);
    assert!(max <= 6, "thumbnail max diff {max}");
    assert!(mean < 1.5, "thumbnail mean diff {mean}");
}

#[test]
fn exif_thumbnail_absent_or_corrupt_returns_none() {
    assert!(exif_thumbnail(&fixture("jpeg/baseline_420.jpg")).is_none());
    let original = fixture("jpeg/exif_thumb.jpg");
    // Corrupt the TIFF byte order and the IFD0 next-IFD pointer.
    let mut bad_order = original.clone();
    let exif_start = bad_order
        .windows(6)
        .position(|w| w == b"Exif\0\0")
        .expect("Exif")
        + 6;
    bad_order[exif_start] = b'X';
    assert!(exif_thumbnail(&bad_order).is_none());
    // Truncating before the APP1/IFD1 payload removes the thumbnail.
    assert!(exif_thumbnail(&original[..64]).is_none());
    assert!(exif_thumbnail(&original[..2]).is_none());
}

#[test]
fn scaled_decode_uses_the_exif_thumbnail_policy() {
    // Base is 320x240 with a 160x120 thumbnail; max_dim=80 requests 80x60 (scale 4),
    // so the thumbnail already has enough resolution and is used instead of the
    // reduced-IDCT decode of the full image.
    let bytes = fixture("jpeg/exif_thumb.jpg");
    let scaled = decode_scaled(&bytes, 80).expect("scaled via thumbnail");
    assert_eq!(scaled.dimensions(), (80, 60));
    let thumbnail = exif_thumbnail(&bytes).expect("thumbnail");
    let expected = nearest_resize(&thumbnail, 80, 60);
    assert_eq!(
        scaled.data, expected,
        "policy must use the thumbnail + exact resize"
    );
    // Deterministic across calls.
    assert_eq!(scaled, decode_scaled(&bytes, 80).expect("scaled again"));
}

#[test]
fn progressive_scaled_decode_falls_back_to_full_resize() {
    let bytes = fixture("jpeg/progressive.jpg");
    let full = decode(&bytes).expect("progressive full decode");
    assert_eq!(full.dimensions(), (12, 8));
    // max_dim=8 → scale 2 → target ceil(12/2) x ceil(8/2) = 6x4.
    let scaled = decode_scaled(&bytes, 8).expect("progressive scaled");
    assert_eq!(scaled.dimensions(), (6, 4));
    assert_eq!(scaled.data, nearest_resize(&full, 6, 4));
}

#[test]
fn scaled_jpeg_rejects_corrupt_or_truncated_input() {
    let bytes = fixture("jpeg/reduced_1024x768.jpg");
    for length in [0usize, 8, 64, 512, bytes.len() / 2] {
        let error = decode_scaled(&bytes[..length], 256).expect_err("truncated");
        assert!(
            matches!(
                error,
                ImageError::Truncated { .. }
                    | ImageError::InvalidData { .. }
                    | ImageError::InvalidSignature
                    | ImageError::CrcMismatch { .. }
            ),
            "length {length}: {error:?}"
        );
    }
}
