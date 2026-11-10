//! CSS `background-repeat`/`background-size`/`background-position` painting
//! (R89): tile geometry, cover/contain, positions, clipping, raster output and
//! the legacy stretch compatibility path.

use std::sync::Arc;

use css::{
    BackgroundPosition, BackgroundRepeat, BackgroundSize, Color, ComputedStyle, Display, Length,
};
use dom::{Document, Namespace};
use image::RgbaImage;
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, ImageStore, MAX_BACKGROUND_TILES, Pixmap, background_tiles,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 200.0,
    height: 120.0,
};

fn node_ids(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, id);
        ids.push(id);
    }
    (document, ids)
}

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn background_style(
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: BackgroundPosition,
) -> ComputedStyle {
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    style.background_repeat = repeat;
    style.background_size = size;
    style.background_position = position;
    style.background_geometry_explicit = true;
    style
}

fn box_with_style(
    id: dom::NodeId,
    style: ComputedStyle,
    rect: Rect,
    padding: EdgeSizes,
) -> LayoutBox {
    let mut layout_box = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    layout_box.dimensions = Dimensions {
        content: rect,
        padding,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

/// Builds `html > div` with a patterned background and paints it.
fn render_background(style: ComputedStyle, image: Arc<RgbaImage>) -> (paint::DisplayList, Pixmap) {
    let (document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE; // keep canvas propagation on root
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children.push(box_with_style(
        ids[1],
        style,
        Rect::new(10.0, 10.0, 40.0, 20.0),
        EdgeSizes::ZERO,
    ));
    let backgrounds: BackgroundImages = [("bg.png".to_string(), image)].into_iter().collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    let pixmap = rasterize_display_list(&display_list, 200, 120);
    let _ = document;
    (display_list, pixmap)
}

fn images(display_list: &paint::DisplayList) -> Vec<Rect> {
    display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn checker() -> Arc<RgbaImage> {
    Arc::new(
        RgbaImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, // red
                0, 255, 0, 255, // green
                0, 0, 255, 255, // blue
                255, 0, 255, 255, // magenta
            ],
        )
        .expect("valid RGBA"),
    )
}

#[test]
fn test_no_repeat_centered_single_tile() {
    let position = BackgroundPosition::parse("center", 16.0, 16.0).expect("center");
    let style = background_style(BackgroundRepeat::NO_REPEAT, BackgroundSize::Auto, position);
    let (display_list, pixmap) = render_background(style, checker());
    let tiles = images(&display_list);
    assert_eq!(tiles.len(), 1);
    let tile = tiles[0];
    // Box padding box is (10, 10, 40, 20), image 2x2 centered.
    assert!((tile.x - 29.0).abs() < 0.01, "tile x = {}", tile.x);
    assert!((tile.y - 19.0).abs() < 0.01, "tile y = {}", tile.y);
    assert_eq!((tile.width, tile.height), (2.0, 2.0));
    assert_eq!(pixmap.pixel(29, 19), Color::RED);
    assert_eq!(pixmap.pixel(30, 20), Color::new(255, 0, 255, 255));
}

#[test]
fn test_cover_and_contain_keep_aspect_ratio() {
    let position = BackgroundPosition::parse("center", 16.0, 16.0).expect("center");
    let cover_style =
        background_style(BackgroundRepeat::NO_REPEAT, BackgroundSize::Cover, position);
    let (display_list, _) = render_background(cover_style, checker());
    let tiles = images(&display_list);
    assert_eq!(tiles.len(), 1);
    assert!(
        (tiles[0].width - 40.0).abs() < 0.01 && (tiles[0].height - 40.0).abs() < 0.01,
        "cover should fill the 40x20 box with a 40x40 square: {:?}",
        tiles[0]
    );
    assert_eq!(tiles[0].x, 10.0);
    assert!(
        (tiles[0].y - 0.0).abs() < 0.01,
        "centered vertically: {:?}",
        tiles[0]
    );

    let contain_style = background_style(
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Contain,
        position,
    );
    let (display_list, _) = render_background(contain_style, checker());
    let tiles = images(&display_list);
    assert_eq!(tiles.len(), 1);
    assert!(
        (tiles[0].width - 20.0).abs() < 0.01 && (tiles[0].height - 20.0).abs() < 0.01,
        "contain should fit the 40x20 box: {:?}",
        tiles[0]
    );
    assert!((tiles[0].x - 20.0).abs() < 0.01 && tiles[0].y == 10.0);
}

#[test]
fn test_repeat_tiles_and_raster_pattern() {
    let style = background_style(
        BackgroundRepeat::REPEAT,
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    let (display_list, pixmap) = render_background(style, checker());
    // 40x20 box with a 2x2 tile: 20 x 10 = 200 tiles (capped far above).
    assert_eq!(images(&display_list).len(), 200);
    // Checker pattern repeats: (0,0) red, (1,0) green, (0,1) blue, (1,1) magenta,
    // then again red/green at (2,0)/(3,0)...
    assert_eq!(pixmap.pixel(10, 10), Color::RED);
    assert_eq!(pixmap.pixel(11, 10), Color::new(0, 255, 0, 255));
    assert_eq!(pixmap.pixel(10, 11), Color::new(0, 0, 255, 255));
    assert_eq!(pixmap.pixel(11, 11), Color::new(255, 0, 255, 255));
    assert_eq!(pixmap.pixel(12, 10), Color::RED);
    // Outside the box the white canvas remains.
    assert_eq!(pixmap.pixel(5, 5), Color::WHITE);
}

#[test]
fn test_repeat_x_only_tiles_along_x() {
    let style = background_style(
        BackgroundRepeat::parse("repeat-x").expect("repeat-x"),
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    let (display_list, _) = render_background(style, checker());
    let tiles = images(&display_list);
    assert_eq!(tiles.len(), 20);
    assert!(tiles.iter().all(|tile| tile.y == 10.0));
}

#[test]
fn test_explicit_size_and_position_pixels() {
    let size = BackgroundSize::Explicit(Length::Px(10.0), Length::Px(4.0));
    let position = BackgroundPosition::parse("5px 6px", 16.0, 16.0).expect("position");
    let style = background_style(BackgroundRepeat::NO_REPEAT, size, position);
    let (display_list, pixmap) = render_background(style, checker());
    let tiles = images(&display_list);
    assert_eq!(tiles, vec![Rect::new(15.0, 16.0, 10.0, 4.0)]);
    // Bilinear scaling keeps the top-left quadrant red at the tile origin.
    assert_eq!(pixmap.pixel(15, 16), Color::RED);
}

#[test]
fn test_background_image_is_clipped_to_padding_box() {
    let mut style = background_style(
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Explicit(Length::Px(100.0), Length::Px(50.0)),
        BackgroundPosition::ZERO,
    );
    style.background_color = Color::new(0, 0, 255, 255);
    let (display_list, pixmap) = render_background(style, checker());
    // Clip items wrap the background layer.
    assert!(display_list.iter().any(|item| matches!(
        item,
        DisplayItem::PushClip { rect } if *rect == Rect::new(10.0, 10.0, 40.0, 20.0)
    )));
    // Image rect is larger than the padding box but clipped to it.
    assert_eq!(
        images(&display_list),
        vec![Rect::new(10.0, 10.0, 100.0, 50.0)]
    );
    // The first quadrant is clamped to pure red; interior pixels are bilinear blends.
    assert_eq!(pixmap.pixel(12, 12), Color::RED);
    assert_ne!(pixmap.pixel(45, 15), Color::WHITE);
    // Outside the box the canvas is white, inside but outside the image (none)
    // the background color is painted underneath.
    assert_eq!(pixmap.pixel(5, 5), Color::WHITE);
}

#[test]
fn test_legacy_stretch_when_geometry_not_authored() {
    let mut style = block_style();
    style.background_image = Some("bg.png".to_string());
    // Defaults: repeat/auto/0% 0%, but not explicitly authored.
    let (display_list, _) = render_background(style, checker());
    let tiles = images(&display_list);
    assert_eq!(
        tiles,
        vec![Rect::new(10.0, 10.0, 40.0, 20.0)],
        "image-only backgrounds keep the historical stretch"
    );
    assert!(
        !display_list
            .iter()
            .any(|item| matches!(item, DisplayItem::PushClip { .. })),
        "legacy stretch does not need a clip item"
    );
}

#[test]
fn test_tile_generation_is_deterministic_and_capped() {
    let style = background_style(
        BackgroundRepeat::REPEAT,
        BackgroundSize::Explicit(Length::Px(0.5), Length::Px(0.5)),
        BackgroundPosition::ZERO,
    );
    let image = checker();
    let first = render_background(style.clone(), Arc::clone(&image)).0;
    let second = render_background(style, image).0;
    assert_eq!(first.items, second.items);
    assert!(images(&first).len() <= MAX_BACKGROUND_TILES);
}

#[test]
fn test_background_tiles_matches_painter_geometry() {
    let padding_box = Rect::new(0.0, 0.0, 100.0, 50.0);
    let position = BackgroundPosition::parse("center", 16.0, 16.0).expect("center");
    let tiles = background_tiles(
        padding_box,
        (20, 20),
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Cover,
        position,
    );
    assert_eq!(tiles, vec![Rect::new(0.0, -25.0, 100.0, 100.0)]);
}
