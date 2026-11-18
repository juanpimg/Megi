use super::common::*;

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
