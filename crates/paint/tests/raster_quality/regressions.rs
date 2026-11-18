use super::common::*;

// --- Regression tests for the fractional-width coverage bug (R46) -----------
//
// The optimized `fill_rect` path splits the row into "partial columns + fully
// covered columns + partial columns". When a rectangle is narrower than one
// pixel (or has fractional x and no fully covered column), `full_begin` and
// `full_end` collapsed to 0 and the trailing partial loop indexed
// `coverage_x[x - ix0]` for x < ix0: debug builds panicked with a subtract
// overflow, release builds indexed out of bounds. These tests fail (panic)
// before the fix and assert exact antialiased coverage after it.

#[test]
fn test_fill_rect_subpixel_width_full_rows_is_antialiased() {
    let mut pixmap = Pixmap::new_with_background(16, 4, Color::WHITE);
    pixmap.fill_rect(Rect::new(10.3, 0.0, 0.4, 4.0), Color::RED);
    for y in 0..4 {
        let covered = pixmap.pixel(10, y);
        assert_eq!(covered.r, 255);
        assert!(
            (140..=170).contains(&covered.g),
            "column 10 should be ~40 % red, got {covered:?}"
        );
        assert_eq!(pixmap.pixel(9, y), Color::WHITE);
        assert_eq!(pixmap.pixel(11, y), Color::WHITE);
    }
}

#[test]
fn test_fill_rect_one_pixel_with_fractional_x_and_full_rows() {
    // Columns 5 and 6 are each half covered; rows 1..3 are fully covered, which
    // is exactly the code path with `coverage_y == 256` and no full column.
    let mut pixmap = Pixmap::new_with_background(12, 6, Color::WHITE);
    pixmap.fill_rect(Rect::new(5.5, 1.0, 1.0, 2.0), Color::RED);
    for y in 1..3 {
        for x in [5u32, 6u32] {
            let pixel = pixmap.pixel(x, y);
            assert_eq!(pixel.r, 255);
            assert!(
                (115..=140).contains(&pixel.g),
                "column {x} row {y} should be ~50 % red, got {pixel:?}"
            );
        }
    }
    assert_eq!(pixmap.pixel(4, 1), Color::WHITE);
    assert_eq!(pixmap.pixel(7, 1), Color::WHITE);
    assert_eq!(pixmap.pixel(5, 0), Color::WHITE);
}

#[test]
fn test_fill_rect_partial_full_partial_area_is_exact() {
    // x = 5.5, width 3: column 5 is half, column 6 is fully covered, column 7
    // is half. Total covered area must stay exactly 3 x 4 = 12 px².
    let mut pixmap = Pixmap::new_with_background(16, 4, Color::WHITE);
    pixmap.fill_rect(Rect::new(5.5, 0.0, 3.0, 4.0), Color::RED);
    for y in 0..4 {
        assert_eq!(pixmap.pixel(6, y), Color::RED, "fully covered column");
    }
    let covered = red_ink_mass(&pixmap);
    assert!(
        (covered - 12.0).abs() < 0.5,
        "mixed partial/full rect must cover 12 px², got {covered:.2}"
    );
}

#[test]
fn test_fill_rect_degenerate_and_non_finite_do_not_paint() {
    let mut pixmap = Pixmap::new_with_background(8, 8, Color::WHITE);
    for rect in [
        Rect::new(2.0, 2.0, 0.0, 5.0),
        Rect::new(2.0, 2.0, 5.0, 0.0),
        Rect::new(2.0, 2.0, -1.0, -1.0),
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
        Rect::new(0.0, f32::INFINITY, 1.0, 1.0),
    ] {
        pixmap.fill_rect(rect, Color::RED);
    }
    assert!(
        pixmap.pixels.iter().all(|pixel| *pixel == Color::WHITE),
        "degenerate/non-finite rectangles must not paint"
    );
}
