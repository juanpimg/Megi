//! Additive scaled-decoding API (`image::decode_scaled`).

use std::fs;
use std::path::PathBuf;

use image::{ImageError, RgbaImage, decode, decode_scaled};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(path).expect("fixture")
}

/// Independent nearest-neighbour row/column subsampling oracle.
fn nearest_sample(image: &RgbaImage, max_dim: u32) -> RgbaImage {
    let factor = image.width.max(image.height).div_ceil(max_dim).max(1);
    let out_width = image.width.div_ceil(factor);
    let out_height = image.height.div_ceil(factor);
    let mut data = Vec::with_capacity((out_width * out_height * 4) as usize);
    for out_y in 0..out_height {
        let source_y = (out_y * factor).min(image.height - 1);
        for out_x in 0..out_width {
            let source_x = (out_x * factor).min(image.width - 1);
            let offset = ((source_y * image.width + source_x) * 4) as usize;
            data.extend_from_slice(&image.data[offset..offset + 4]);
        }
    }
    RgbaImage::new(out_width, out_height, data).expect("oracle image")
}

#[test]
fn scaled_png_dimensions_for_factors_one_two_four_eight() {
    let bytes = fixture("png/large_800x600.png");
    let full = decode(&bytes).expect("full decode");
    assert_eq!((full.width, full.height), (800, 600));
    for (max_dim, expected) in [
        (800u32, (800u32, 600u32)),
        (400, (400, 300)),
        (200, (200, 150)),
        (100, (100, 75)),
    ] {
        let scaled = decode_scaled(&bytes, max_dim).expect("scaled decode");
        assert_eq!((scaled.width, scaled.height), expected, "max_dim={max_dim}");
    }
}

#[test]
fn scaled_png_matches_row_column_subsampling() {
    let bytes = fixture("png/large_800x600.png");
    let full = decode(&bytes).expect("full decode");
    for max_dim in [400u32, 200, 100, 64] {
        let scaled = decode_scaled(&bytes, max_dim).expect("scaled");
        assert_eq!(scaled, nearest_sample(&full, max_dim), "max_dim={max_dim}");
    }
}

#[test]
fn scaled_jpeg_gif_and_webp_match_post_subsampling() {
    for (name, max_dim) in [
        ("jpeg/baseline_420.jpg", 16u32),
        ("jpeg/progressive.jpg", 8),
        ("gif/palette.gif", 8),
        ("webp/photo_transforms.webp", 24),
        ("webp/palette_real.webp", 16),
        ("webp/alpha_real.webp", 20),
    ] {
        let bytes = fixture(name);
        let full = decode(&bytes).expect(name);
        let scaled = decode_scaled(&bytes, max_dim).expect(name);
        assert_eq!(
            scaled,
            nearest_sample(&full, max_dim),
            "{name} max_dim={max_dim}"
        );
        assert!(
            scaled.width <= max_dim && scaled.height <= max_dim,
            "{name}"
        );
    }
}

#[test]
fn max_dim_larger_than_image_is_identity() {
    let bytes = fixture("png/rgba8.png");
    let full = decode(&bytes).expect("full decode");
    let unchanged = decode_scaled(&bytes, 4096).expect("scaled identity");
    assert_eq!(unchanged, full);
}

#[test]
fn zero_max_dim_is_a_typed_error() {
    let bytes = fixture("png/rgba8.png");
    let error = decode_scaled(&bytes, 0).expect_err("max_dim 0");
    assert!(
        matches!(error, ImageError::InvalidData { .. }),
        "unexpected: {error:?}"
    );
}

#[test]
fn corrupt_and_truncated_scaled_inputs_are_typed_errors() {
    let bytes = fixture("png/large_800x600.png");
    for length in [0usize, 8, 32, 128, bytes.len() / 2] {
        let error = decode_scaled(&bytes[..length], 128).expect_err("truncated");
        assert!(
            matches!(
                error,
                ImageError::Truncated { .. }
                    | ImageError::InvalidData { .. }
                    | ImageError::CrcMismatch { .. }
                    | ImageError::InvalidSignature
            ),
            "length {length}: {error:?}"
        );
    }
    let mut corrupt = bytes.clone();
    corrupt[0] = b'X';
    assert!(matches!(
        decode_scaled(&corrupt, 128),
        Err(ImageError::InvalidSignature)
    ));
}

#[test]
fn scaled_decode_is_deterministic() {
    let bytes = fixture("jpeg/baseline_420.jpg");
    let first = decode_scaled(&bytes, 12).expect("first");
    let second = decode_scaled(&bytes, 12).expect("second");
    assert_eq!(first, second);
}
