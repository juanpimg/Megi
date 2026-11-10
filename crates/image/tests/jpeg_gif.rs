//! JPEG baseline and GIF first-frame decode tests (R133).
//!
//! Fixtures are synthetic images generated with python3/PIL (own content, no external
//! material); JPEG is lossy so colour assertions use a small tolerance, while the uniform
//! quadrant/block fixtures keep the expected colours well separated.

use std::path::Path;

use image::{ImageError, ImageFormat, decode, detect_format};

fn fixture(relative: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("fixture {}: {error}", path.display()))
}

fn close(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| a.abs_diff(*b) <= tolerance)
}

#[test]
fn detects_jpeg_and_gif_signatures() {
    assert_eq!(
        detect_format(&fixture("jpeg/baseline_420.jpg")),
        ImageFormat::Jpeg
    );
    assert_eq!(detect_format(&fixture("gif/palette.gif")), ImageFormat::Gif);
}

#[test]
fn decodes_baseline_jpeg_subsampling_variants() {
    for name in ["baseline_444.jpg", "baseline_422.jpg", "baseline_420.jpg"] {
        let image = decode(&fixture(&format!("jpeg/{name}"))).expect(name);
        assert_eq!(image.dimensions(), (32, 32), "{name}");
        // Uniform quadrants: centre pixel of each quadrant is close to the source colour.
        // Quadrants are 16x16 (MCU-aligned for 4:2:0/4:2:2) so chroma subsampling cannot
        // bleed across the colour boundaries.
        let quadrants = [
            ((8, 8), [220, 30, 30, 255]),
            ((24, 8), [30, 200, 30, 255]),
            ((8, 24), [30, 30, 220, 255]),
            ((24, 24), [240, 220, 30, 255]),
        ];
        for ((x, y), expected) in quadrants {
            let actual = image.pixel(x, y).expect("pixel in bounds");
            assert!(
                close(actual, expected, 14),
                "{name} at ({x},{y}): got {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn decodes_grayscale_jpeg() {
    let image = decode(&fixture("jpeg/grayscale.jpg")).expect("grayscale jpeg");
    assert_eq!(image.dimensions(), (16, 12));
    for (x, y) in [(0, 0), (15, 0), (0, 11), (15, 11)] {
        let [r, g, b, a] = image.pixel(x, y).expect("pixel");
        assert_eq!(
            (r, g, b),
            (r, r, r),
            "grayscale must map to equal RGB at ({x},{y})"
        );
        assert_eq!(a, 255);
    }
    let expected = (15u32 * 8 + 11 * 4) as u8;
    let actual = image.pixel(15, 11).expect("pixel")[0];
    assert!(
        actual.abs_diff(expected) <= 8,
        "grayscale sample: got {actual}, expected ~{expected}"
    );
}

#[test]
fn progressive_jpeg_is_rejected_with_a_clear_error() {
    let error = decode(&fixture("jpeg/progressive.jpg")).expect_err("progressive must fail");
    match error {
        ImageError::UnsupportedFeature { feature } => {
            assert!(feature.contains("progressive"), "feature: {feature}");
        }
        other => panic!("expected UnsupportedFeature, got {other:?}"),
    }
}

#[test]
fn corrupt_jpeg_never_panics_and_is_typed() {
    let error = decode(&fixture("jpeg/corrupt.jpg")).expect_err("truncated JPEG must fail");
    assert!(
        matches!(
            error,
            ImageError::Truncated { .. } | ImageError::InvalidData { .. }
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn decodes_gif_palette_and_first_frame() {
    let image = decode(&fixture("gif/palette.gif")).expect("palette gif");
    assert_eq!(image.dimensions(), (20, 12));
    let expected = [
        ((2, 2), [255, 0, 0, 255]),
        ((7, 2), [0, 255, 0, 255]),
        ((12, 2), [0, 0, 255, 255]),
        ((17, 2), [255, 255, 0, 255]),
    ];
    for ((x, y), color) in expected {
        assert_eq!(image.pixel(x, y), Some(color), "GIF colour at ({x},{y})");
    }
}

#[test]
fn decodes_gif_transparency_and_frame_offset() {
    let image = decode(&fixture("gif/transparent.gif")).expect("transparent gif");
    assert_eq!(image.dimensions(), (20, 12));
    // Pixels outside the 8x6 frame at (4,3) are transparent.
    assert_eq!(
        image.pixel(0, 0).expect("pixel")[3],
        0,
        "outside frame is transparent"
    );
    assert_eq!(image.pixel(19, 11).expect("pixel")[3], 0);
    // Inside the frame the transparent palette index 0 is honoured (alpha 0), while other
    // palette indices are opaque.
    let inside = image.pixel(6, 5).expect("pixel in frame");
    assert_eq!(inside[3], 255, "coloured frame pixel is opaque: {inside:?}");
}

#[test]
fn animated_gif_roundtrip_uses_the_first_frame() {
    let image = decode(&fixture("gif/animated.gif")).expect("animated gif");
    assert_eq!(image.dimensions(), (12, 8));
    // First frame is palette index 1 (pure green in the fixture palette).
    assert_eq!(image.pixel(6, 4), Some([0, 255, 0, 255]));
}
