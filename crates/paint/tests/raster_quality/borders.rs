use super::common::*;

#[test]
fn test_border_sides_use_their_own_colors() {
    let top = Color::new(255, 0, 0, 255);
    let right = Color::new(0, 255, 0, 255);
    let bottom = Color::new(0, 0, 255, 255);
    let left = Color::new(255, 255, 0, 255);
    // border box at (10, 10), 100x60; sides: top 4, right 6, bottom 2, left 8.
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(10.0, 10.0, 100.0, 60.0),
        sides: [
            side(4.0, BorderStyle::Solid, top),
            side(6.0, BorderStyle::Solid, right),
            side(2.0, BorderStyle::Solid, bottom),
            side(8.0, BorderStyle::Solid, left),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        120,
        80,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    assert_eq!(pixmap.pixel(50, 11), top, "top band");
    assert_eq!(pixmap.pixel(106, 40), right, "right band");
    assert_eq!(pixmap.pixel(50, 69), bottom, "bottom band");
    assert_eq!(pixmap.pixel(13, 40), left, "left band");
    // Inside the border box is untouched (transparent border interior).
    assert_eq!(pixmap.pixel(60, 40), Color::WHITE);
}

#[test]
fn test_border_corners_are_mitred() {
    let top = Color::new(255, 0, 0, 255);
    let left = Color::new(255, 255, 0, 255);
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(10.0, 10.0, 100.0, 60.0),
        sides: [
            side(8.0, BorderStyle::Solid, top),
            BorderSide::NONE,
            BorderSide::NONE,
            side(8.0, BorderStyle::Solid, left),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        120,
        80,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    // Top-left corner square is 8x8 at (10, 10); the diagonal goes from (10,10)
    // to (18,18). Above/right of it is top (red), below/left is left (yellow).
    assert_eq!(pixmap.pixel(16, 11), top, "upper-right of the mitre is top");
    assert_eq!(
        pixmap.pixel(11, 16),
        left,
        "lower-left of the mitre is left"
    );
    // The diagonal pixels are antialiased blends of both colors.
    let diagonal = pixmap.pixel(14, 14);
    assert_ne!(diagonal, top);
    assert_ne!(diagonal, left);
    assert!(
        diagonal.r == 255 && diagonal.g > 0 && diagonal.g < 255,
        "diagonal pixel must mix red and yellow, got {diagonal:?}"
    );
}

#[test]
fn test_border_styles_dash_dot_double() {
    let red = Color::RED;
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(0.0, 0.0, 60.0, 20.0),
        sides: [
            side(2.0, BorderStyle::Dashed, red),
            BorderSide::NONE,
            side(2.0, BorderStyle::Dotted, red),
            BorderSide::NONE,
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        60,
        20,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    // Dashed top band: painted runs interrupted by gaps.
    let row: Vec<bool> = (0..60).map(|x| pixmap.pixel(x, 1) == red).collect();
    let painted = row.iter().filter(|painted| **painted).count();
    assert!(painted > 0 && painted < 60, "dashed top must have gaps");
    let runs = row.windows(2).filter(|pair| !pair[0] && pair[1]).count();
    assert!(runs >= 2, "dashed top must have at least two dashes");

    // Dotted bottom band: small dots separated by larger gaps.
    let dot_row: Vec<bool> = (0..60).map(|x| pixmap.pixel(x, 18) == red).collect();
    let dot_painted = dot_row.iter().filter(|painted| **painted).count();
    assert!(
        dot_painted > 0 && dot_painted < 30,
        "dotted bottom has dots"
    );

    // Double: two thin lines with a gap, on a 6 px top border.
    let mut double_list = DisplayList::new();
    double_list.push(DisplayItem::Border {
        rect: Rect::new(0.0, 0.0, 40.0, 20.0),
        sides: [
            side(6.0, BorderStyle::Double, red),
            BorderSide::NONE,
            BorderSide::NONE,
            BorderSide::NONE,
        ],
    });
    let double = rasterize_display_list_with_font_database(
        &double_list,
        40,
        20,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert_eq!(double.pixel(20, 1), red, "first line of double border");
    assert_eq!(double.pixel(20, 3), Color::WHITE, "gap of double border");
    assert_eq!(double.pixel(20, 5), red, "second line of double border");
}

#[test]
fn test_border_none_and_hidden_paint_nothing() {
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(5.0, 5.0, 30.0, 20.0),
        sides: [
            side(4.0, BorderStyle::None, Color::RED),
            side(4.0, BorderStyle::Hidden, Color::BLUE),
            side(4.0, BorderStyle::None, Color::GREEN),
            side(4.0, BorderStyle::Hidden, Color::BLACK),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        40,
        30,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert!(
        pixmap.pixels.iter().all(|pixel| *pixel == Color::WHITE),
        "border styles none/hidden must not paint"
    );
}
