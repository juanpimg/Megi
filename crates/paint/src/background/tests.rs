use super::*;

fn padding(width: f32, height: f32) -> Rect {
    Rect::new(0.0, 0.0, width, height)
}

#[test]
fn test_round_with_a_sub_pixel_tile_terminates() {
    // `positioning_length / tile` is infinite for a 1e-38 px tile; the repeat count
    // used to saturate `usize` and the zero-size tiles never intersected the clip.
    let (tile, origins) = plan_axis(
        0.0,
        100.0,
        0.0,
        100.0,
        Length::Px(0.0),
        1e-38,
        BackgroundRepeatAxis::Round,
        64,
    );
    assert!(tile.is_finite(), "{tile}");
    assert!(origins.len() <= 64);
}

#[test]
fn test_space_with_a_sub_pixel_tile_terminates() {
    // The origin is far outside the clip on purpose: with an intersecting origin the old
    // loop stopped at `origins.len() == limit`, so it did not expose the `count` hang.
    let (_, origins) = plan_axis(
        1e6,
        100.0,
        0.0,
        100.0,
        Length::Px(0.0),
        1e-38,
        BackgroundRepeatAxis::Space,
        64,
    );
    assert!(origins.is_empty(), "a degenerate space tile paints nothing");
}

#[test]
fn test_round_truncation_starts_at_the_clip() {
    // The clip is far from the positioning origin: the first `limit` tile indices do not
    // intersect it, so the window must start at the first visible tile (index 4500).
    let (tile, origins) = plan_axis(
        0.0,
        100.0,
        90.0,
        10.0,
        Length::Px(0.0),
        0.02,
        BackgroundRepeatAxis::Round,
        10,
    );
    assert!((tile - 0.02).abs() < 1e-6, "{tile}");
    assert_eq!(origins.len(), 10);
    assert!((origins[0] - 90.0).abs() < 1e-3, "{:?}", origins[0]);
}

#[test]
fn test_space_truncation_starts_at_the_clip() {
    let (_, origins) = plan_axis(
        0.0,
        100.0,
        90.0,
        10.0,
        Length::Px(0.0),
        1.0,
        BackgroundRepeatAxis::Space,
        8,
    );
    assert!(!origins.is_empty(), "visible space tiles must be painted");
    assert!(origins[0] >= 89.0, "{:?}", origins[0]);
}

#[test]
fn test_round_with_a_finite_count_over_the_limit_still_paints() {
    // 100 / 0.02 = 5,000 tiles > limit: the tiles that fit the cap must still be
    // emitted (an earlier guard dropped every tile whenever count > limit).
    let (tile, origins) = plan_axis(
        0.0,
        100.0,
        0.0,
        100.0,
        Length::Px(0.0),
        0.02,
        BackgroundRepeatAxis::Round,
        10,
    );
    assert!((tile - 0.02).abs() < 1e-6, "{tile}");
    assert_eq!(origins.len(), 10);
}

#[test]
fn test_round_on_the_second_axis_keeps_tiles_after_the_first_consumes_the_budget() {
    // 800×600 with a 10×10 image: x uses 80 tiles, leaving a y limit of 51 of its 60
    // tiles. The y axis must still paint 51 rows (it used to return none).
    let repeat = BackgroundRepeat {
        x: BackgroundRepeatAxis::Round,
        y: BackgroundRepeatAxis::Round,
    };
    let tiles = background_tiles(
        padding(800.0, 600.0),
        (10, 10),
        repeat,
        BackgroundSize::Auto,
        BackgroundPosition::parse("0 0", 10.0, 10.0).unwrap(),
    );
    assert_eq!(tiles.len(), 80 * 51);
}

#[test]
fn test_repeat_with_huge_offsets_does_not_overflow() {
    let (tile, origins) = plan_axis(
        1e38,
        100.0,
        0.0,
        100.0,
        Length::Px(1e38),
        1.0,
        BackgroundRepeatAxis::Repeat,
        64,
    );
    assert!(tile.is_finite());
    assert!(origins.len() <= 64);
}

#[test]
fn test_no_repeat_centered() {
    let tiles = background_tiles(
        padding(100.0, 50.0),
        (16, 16),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Auto,
        BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
    );
    assert_eq!(tiles, vec![Rect::new(42.0, 17.0, 16.0, 16.0)]);
}

#[test]
fn test_cover_and_contain_preserve_aspect_ratio() {
    let cover = background_tiles(
        padding(100.0, 50.0),
        (20, 20),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Cover,
        BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
    );
    assert_eq!(cover, vec![Rect::new(0.0, -25.0, 100.0, 100.0)]);
    let contain = background_tiles(
        padding(100.0, 50.0),
        (20, 20),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Contain,
        BackgroundPosition::parse("center", 16.0, 16.0).unwrap(),
    );
    assert_eq!(contain, vec![Rect::new(25.0, 0.0, 50.0, 50.0)]);
}

#[test]
fn test_repeat_axis_counts() {
    let both = background_tiles(
        padding(12.0, 12.0),
        (4, 4),
        BackgroundRepeat::REPEAT,
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    assert_eq!(both.len(), 9);
    let x_only = background_tiles(
        padding(12.0, 12.0),
        (4, 4),
        BackgroundRepeat::parse("repeat-x").unwrap(),
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    assert_eq!(x_only.len(), 3);
    assert!(x_only.iter().all(|tile| tile.y == 0.0));
}

#[test]
fn test_explicit_sizes_and_positions() {
    let half = background_tiles(
        padding(100.0, 50.0),
        (20, 20),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::parse("50%", 16.0, 16.0).unwrap(),
        BackgroundPosition::parse("50%", 16.0, 16.0).unwrap(),
    );
    assert_eq!(half, vec![Rect::new(25.0, 12.5, 50.0, 25.0)]);
    let px = background_tiles(
        padding(100.0, 50.0),
        (20, 20),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::parse("100px 10px", 16.0, 16.0).unwrap(),
        BackgroundPosition::parse("10px 0", 16.0, 16.0).unwrap(),
    );
    assert_eq!(px, vec![Rect::new(10.0, 0.0, 100.0, 10.0)]);
}

#[test]
fn test_space_distributes_equal_gaps() {
    // 10px tile in a 45px area: 4 tiles, 3 gaps of (45-40)/3.
    let tiles = background_tiles(
        padding(45.0, 10.0),
        (10, 10),
        BackgroundRepeat::parse("space").unwrap(),
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    assert_eq!(tiles.len(), 4);
    assert_eq!(tiles[0].x, 0.0);
    assert_eq!(tiles[1].x, 10.0 + 5.0 / 3.0);
    assert!((tiles[3].x + tiles[3].width - 45.0).abs() < 1e-4);
}

#[test]
fn test_round_scales_tile_to_fit() {
    // 10px tile in a 45px area => round(4.5) = 5 tiles of 9px.
    let tiles = background_tiles(
        padding(45.0, 10.0),
        (10, 10),
        BackgroundRepeat::parse("round").unwrap(),
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    assert_eq!(tiles.len(), 5);
    assert!((tiles[0].width - 9.0).abs() < 1e-4);
    assert!((tiles[4].x + tiles[4].width - 45.0).abs() < 1e-4);
}

#[test]
fn test_origin_box_selects_positioning_area() {
    let border = Rect::new(0.0, 0.0, 100.0, 50.0);
    let padding_box = Rect::new(5.0, 5.0, 90.0, 40.0);
    let content = Rect::new(10.0, 10.0, 80.0, 30.0);
    assert_eq!(
        background_origin_rect(BackgroundOrigin::ContentBox, border, padding_box, content),
        content
    );
    assert_eq!(
        background_clip_rect(BackgroundClip::PaddingBox, border, padding_box, content),
        padding_box
    );
    assert_eq!(
        background_clip_rect(BackgroundClip::Text, border, padding_box, content),
        border,
        "background-clip: text falls back to the border box"
    );
}
