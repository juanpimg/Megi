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
    let (_document, ids) = node_ids(2);
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

// --- R94: origin/clip/attachment, multiple layers and space/round -------------

use css::{BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin};

fn layer(
    image: &str,
    repeat: BackgroundRepeat,
    size: BackgroundSize,
    position: BackgroundPosition,
) -> BackgroundLayer {
    BackgroundLayer {
        image: Some(image.to_string()),
        repeat,
        size,
        position,
        ..BackgroundLayer::default()
    }
}

fn styled_layers(layers: Vec<BackgroundLayer>) -> ComputedStyle {
    let mut style = block_style();
    style.background_layers = layers;
    style.background_geometry_explicit = true;
    style
}

#[test]
fn test_multiple_layers_are_painted_bottom_to_top() {
    let mut bottom = layer(
        "bottom.png",
        BackgroundRepeat::REPEAT,
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    );
    bottom.origin = BackgroundOrigin::PaddingBox;
    let mut top = layer(
        "top.png",
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Explicit(Length::Px(20.0), Length::Px(20.0)),
        BackgroundPosition::ZERO,
    );
    top.origin = BackgroundOrigin::PaddingBox;
    let style = styled_layers(vec![top, bottom]);

    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children.push(box_with_style(
        ids[1],
        style,
        Rect::new(10.0, 10.0, 40.0, 20.0),
        EdgeSizes::ZERO,
    ));
    let bottom_image = checker();
    let top_image = Arc::new(RgbaImage::new(1, 1, vec![0, 0, 0, 255]).expect("RGBA"));
    let backgrounds: BackgroundImages = [
        ("bottom.png".to_string(), bottom_image),
        ("top.png".to_string(), top_image.clone()),
    ]
    .into_iter()
    .collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    let tiles = images(&display_list);
    // Bottom layer (checker repeat, 40x20 / 2x2 = 200 tiles) comes first, then
    // the single top layer image.
    assert_eq!(tiles.len(), 201);
    assert_eq!(
        tiles.last().copied(),
        Some(Rect::new(10.0, 10.0, 20.0, 20.0))
    );
    let pixmap = rasterize_display_list(&display_list, 200, 120);
    assert_eq!(pixmap.pixel(15, 15), Color::BLACK, "top layer wins");
}

#[test]
fn test_origin_content_box_is_the_positioning_area() {
    let mut style = styled_layers(vec![layer(
        "bg.png",
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::parse("50% 50%", 16.0, 16.0).expect("size"),
        BackgroundPosition::ZERO,
    )]);
    style.background_layers[0].origin = BackgroundOrigin::ContentBox;

    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = box_with_style(
        ids[1],
        style,
        Rect::new(8.0, 8.0, 40.0, 20.0),
        EdgeSizes::new(5.0, 5.0, 5.0, 5.0),
    );
    child.dimensions.border = EdgeSizes::new(3.0, 3.0, 3.0, 3.0);
    root.children.push(child);
    let backgrounds: BackgroundImages = [("bg.png".to_string(), checker())].into_iter().collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    // Content box is (8,8,40,20); 50% size is 20x10 positioned at its origin.
    assert_eq!(images(&display_list), vec![Rect::new(8.0, 8.0, 20.0, 10.0)]);
}

#[test]
fn test_clip_content_box_and_fixed_attachment() {
    let mut style = styled_layers(vec![layer(
        "bg.png",
        BackgroundRepeat::NO_REPEAT,
        BackgroundSize::Explicit(Length::Px(20.0), Length::Px(20.0)),
        BackgroundPosition::ZERO,
    )]);
    style.background_layers[0].clip = BackgroundClip::ContentBox;
    style.background_layers[0].attachment = BackgroundAttachment::Fixed;

    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = box_with_style(
        ids[1],
        style,
        Rect::new(8.0, 8.0, 40.0, 20.0),
        EdgeSizes::new(5.0, 5.0, 5.0, 5.0),
    );
    child.dimensions.border = EdgeSizes::new(3.0, 3.0, 3.0, 3.0);
    root.children.push(child);
    let backgrounds: BackgroundImages = [("bg.png".to_string(), checker())].into_iter().collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    // Fixed attachment positions against the viewport: tile at (0,0,20,20).
    assert_eq!(images(&display_list), vec![Rect::new(0.0, 0.0, 20.0, 20.0)]);
    // Clip is the content box (8,8,40,20).
    assert!(display_list.iter().any(|item| matches!(
        item,
        DisplayItem::PushClip { rect } if *rect == Rect::new(8.0, 8.0, 40.0, 20.0)
    )));
}

#[test]
fn test_round_and_space_painter_geometry() {
    let mut round_style = styled_layers(vec![layer(
        "bg.png",
        BackgroundRepeat::parse("round").expect("round"),
        BackgroundSize::Auto,
        BackgroundPosition::ZERO,
    )]);
    round_style.background_image = Some("bg.png".to_string());
    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = box_with_style(
        ids[1],
        round_style,
        Rect::new(0.0, 0.0, 45.0, 10.0),
        EdgeSizes::ZERO,
    );
    child.dimensions.content = Rect::new(0.0, 0.0, 45.0, 10.0);
    root.children.push(child);
    let image = Arc::new(RgbaImage::new(10, 10, vec![255u8; 400]).expect("RGBA"));
    let backgrounds: BackgroundImages = [("bg.png".to_string(), image)].into_iter().collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    let tiles = images(&display_list);
    assert_eq!(tiles.len(), 5, "round fits 5 rescaled tiles");
    assert!((tiles[0].width - 9.0).abs() < 1e-4);
}

#[test]
fn test_missing_layer_url_is_skipped() {
    let style = styled_layers(vec![
        layer(
            "missing.png",
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        ),
        layer(
            "present.png",
            BackgroundRepeat::NO_REPEAT,
            BackgroundSize::Auto,
            BackgroundPosition::ZERO,
        ),
    ]);
    let (_document, ids) = node_ids(2);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(ids[0]), Some(root_style));
    root.dimensions.content = VIEWPORT;
    root.children.push(box_with_style(
        ids[1],
        style,
        Rect::new(10.0, 10.0, 20.0, 20.0),
        EdgeSizes::ZERO,
    ));
    let backgrounds: BackgroundImages = [("present.png".to_string(), checker())]
        .into_iter()
        .collect();
    let display_list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &backgrounds,
    );
    assert_eq!(images(&display_list), vec![Rect::new(10.0, 10.0, 2.0, 2.0)]);
}
