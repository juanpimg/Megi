//! Submodule of [`super`]: unit tests for the rasterizer.

use super::*;

fn canvas(width: usize, height: usize) -> Canvas {
    Canvas {
        width,
        height,
        pixels: vec![0; width * height * 4],
        coverage: vec![0.0; width * height],
    }
}

#[test]
fn box_blur_preserves_a_constant_color() {
    // Rounding each pass to nearest (instead of truncating) keeps a flat
    // layer unchanged across all six passes (#1442).
    let (width, height) = (8usize, 8usize);
    let mut pixels = vec![0u8; width * height * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        *pixel = [100, 150, 200, 255];
    }
    box_blur(&mut pixels, width, height, 2.0);
    for pixel in pixels.as_chunks::<4>().0 {
        assert_eq!(pixel, &[100, 150, 200, 255]);
    }
}

#[test]
fn test_textpath_segment_construction_and_lookup() {
    // Production construction (the same helper `draw_text` uses) must produce
    // contiguous segments and cumulative ends.
    let subpaths = vec![SubPath {
        points: vec![(0.0, 0.0), (3.0, 0.0), (3.0, 4.0)],
        closed: false,
    }];
    let (segments, ends) = text_path_segments(&subpaths);
    assert_eq!(segments.len(), 2);
    assert_eq!(ends, vec![3.0, 7.0]);
    assert_eq!(segment_at(&segments, &ends, 0.0), Some(segments[0]));
    assert_eq!(segment_at(&segments, &ends, 3.0), Some(segments[0]));
    assert_eq!(segment_at(&segments, &ends, 3.5), Some(segments[1]));
    assert_eq!(segment_at(&segments, &ends, 7.0), Some(segments[1]));

    // Out-of-range and non-finite distances never match.
    for distance in [-1.0, 7.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(segment_at(&segments, &ends, distance), None, "{distance}");
    }

    // Differential check against the original linear scan, including distances that
    // decrease (negative letter spacing / anchors) and exact boundaries.
    let mut many: Vec<TextPathSegment> = Vec::new();
    let mut many_ends: Vec<f32> = Vec::new();
    let mut start = 0.0f32;
    for index in 0..1_000 {
        let length = 1.0 + (index % 3) as f32;
        many.push((start, (index as f32, 0.0), (index as f32 + length, 0.0)));
        start += length;
        many_ends.push(start);
    }
    for step in 0..4_000 {
        let distance = if step % 2 == 0 {
            step as f32 * 0.75
        } else {
            1_999.0 - step as f32 * 0.9
        };
        let expected = many
            .iter()
            .find(|(segment_start, from, to)| {
                let length = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
                distance >= *segment_start && distance <= *segment_start + length
            })
            .copied();
        assert_eq!(
            segment_at(&many, &many_ends, distance),
            expected,
            "{distance}"
        );
    }
}

#[test]
fn test_span_coverage_is_analytic() {
    let mut row = vec![0.0f32; 4];
    add_span_coverage(0.25, 2.75, 4, &mut row);
    assert_eq!(row[0], 0.1875);
    assert!((row[1] - 0.25).abs() < 1e-6);
    assert!((row[2] - 0.1875).abs() < 1e-6);
    assert_eq!(row[3], 0.0);
}

#[test]
fn test_polygon_and_disc_stamps_are_antialiased() {
    let mut coverage = vec![0.0f32; 4];
    stamp_polygon(
        &[(0.5, 0.0), (2.0, 0.0), (2.0, 2.0), (0.5, 2.0)],
        2,
        2,
        &mut coverage,
    );
    assert!(coverage[0] > 0.0 && coverage[0] < 1.0);
    assert_eq!(coverage[1], 1.0);
    let mut disc = vec![0.0f32; 4];
    stamp_disc((1.0, 1.0), 0.5, 2, 2, &mut disc);
    assert!(disc[0] > 0.0);
}

#[test]
fn test_composite_straight_alpha() {
    let mut canvas = canvas(1, 1);
    canvas.coverage[0] = 0.5;
    canvas.composite(Color::rgba(255, 0, 0, 255), 1.0);
    assert_eq!(canvas.pixels[3], 128);
}
