use super::common::*;

#[test]
fn test_fill_rect_antialiases_fractional_edges() {
    let mut pixmap = Pixmap::new_with_background(4, 1, Color::WHITE);
    pixmap.fill_rect(Rect::new(1.5, 0.0, 2.0, 1.0), Color::RED);

    // Column 0 is untouched; columns 1 and 3 are half covered; column 2 is full.
    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
    let left = pixmap.pixel(1, 0);
    let middle = pixmap.pixel(2, 0);
    let right = pixmap.pixel(3, 0);
    assert_eq!(middle, Color::RED);
    for edge in [left, right] {
        assert_eq!(edge.r, 255);
        assert_eq!(edge.a, 255);
        assert!(
            (120..=135).contains(&edge.g),
            "half-covered edge should blend ~50 % red over white, got {edge:?}"
        );
        assert_eq!(edge.g, edge.b);
    }
}

#[test]
fn test_fill_rect_fractional_corner_has_quarter_coverage() {
    let mut pixmap = Pixmap::new_with_background(3, 3, Color::WHITE);
    pixmap.fill_rect(Rect::new(1.5, 1.5, 1.0, 1.0), Color::RED);

    // A 1x1 rect straddling the (1.5, 1.5) corner covers a quarter of each of
    // the four pixels around it.
    for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
        let corner = pixmap.pixel(x, y);
        assert!(
            (180..=205).contains(&corner.g),
            "quarter-covered pixel ({x}, {y}) should be ~25 % red, got {corner:?}"
        );
    }
    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
    assert_eq!(pixmap.pixel(2, 0), Color::WHITE);
}

#[test]
fn test_fractional_rect_area_is_not_overpainted() {
    // Classic finding F-13: fill_rect(2.5, 2.5, 5, 5) painted 36 px instead of
    // the exact 25 px area because of floor/ceil round-out.
    let mut pixmap = Pixmap::new_with_background(12, 12, Color::WHITE);
    pixmap.fill_rect(Rect::new(2.5, 2.5, 5.0, 5.0), Color::RED);
    let covered = red_ink_mass(&pixmap);
    assert!(
        (covered - 25.0).abs() < 1.0,
        "fractional rect must cover its 25 px² area, got {covered:.2} px²"
    );
}

#[test]
fn test_integer_rect_is_still_exactly_opaque() {
    let mut pixmap = Pixmap::new_with_background(8, 8, Color::WHITE);
    pixmap.fill_rect(Rect::new(2.0, 3.0, 4.0, 2.0), Color::RED);
    for y in 3..5 {
        for x in 2..6 {
            assert_eq!(pixmap.pixel(x, y), Color::RED, "pixel ({x}, {y})");
        }
    }
    assert_eq!(pixmap.pixel(1, 3), Color::WHITE);
    assert_eq!(pixmap.pixel(6, 3), Color::WHITE);
}
