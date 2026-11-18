use super::common::*;

#[test]
fn test_clip_limits_fill_and_set_pixel() {
    let mut pixmap = Pixmap::new_with_background(20, 20, Color::WHITE);
    pixmap.push_clip(Rect::new(5.0, 5.0, 10.0, 10.0));
    pixmap.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::RED);
    for y in 5..15 {
        for x in 5..15 {
            assert_eq!(pixmap.pixel(x, y), Color::RED, "inside clip ({x}, {y})");
        }
    }
    assert_eq!(pixmap.pixel(4, 10), Color::WHITE, "outside clip");
    assert_eq!(pixmap.pixel(10, 15), Color::WHITE, "outside clip");

    pixmap.set_pixel(2, 2, Color::BLUE);
    assert_eq!(
        pixmap.pixel(2, 2),
        Color::WHITE,
        "set_pixel must respect clip"
    );
    pixmap.set_pixel(6, 6, Color::BLUE);
    assert_eq!(pixmap.pixel(6, 6), Color::BLUE);

    pixmap.pop_clip();
    pixmap.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::GREEN);
    assert_eq!(pixmap.pixel(2, 2), Color::GREEN, "clip popped");
}

#[test]
fn test_pop_clip_on_empty_stack_is_safe() {
    let mut pixmap = Pixmap::new_with_background(4, 4, Color::WHITE);
    pixmap.pop_clip();
    pixmap.pop_clip();
    pixmap.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::RED);
    assert_eq!(pixmap.pixel(0, 0), Color::RED);
}

#[test]
fn test_pixmap_rejects_absurd_dimensions() {
    // Oversized dimensions yield an empty pixmap instead of panicking (issue #667).
    let pixmap = Pixmap::new(u32::MAX, u32::MAX);
    assert_eq!(pixmap.width, 0);
    assert_eq!(pixmap.height, 0);
    assert!(pixmap.pixels.is_empty());
}
