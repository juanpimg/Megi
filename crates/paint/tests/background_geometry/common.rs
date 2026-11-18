pub(crate) use std::sync::Arc;

pub(crate) use megi_core::{Document, Namespace};
pub(crate) use megi_css::{
    BackgroundPosition, BackgroundRepeat, BackgroundSize, Color, ComputedStyle, Display, Length,
};
pub(crate) use megi_image::RgbaImage;
pub(crate) use megi_layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
pub(crate) use megi_paint::{
    BackgroundImages, DisplayItem, ImageStore, MAX_BACKGROUND_TILES, Pixmap, background_tiles,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

pub(crate) const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 200.0,
    height: 120.0,
};

pub(crate) fn node_ids(count: usize) -> (Document, Vec<megi_core::NodeId>) {
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

pub(crate) fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

pub(crate) fn background_style(
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

pub(crate) fn box_with_style(
    id: megi_core::NodeId,
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
pub(crate) fn render_background(
    style: ComputedStyle,
    image: Arc<RgbaImage>,
) -> (megi_paint::DisplayList, Pixmap) {
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

pub(crate) fn images(display_list: &megi_paint::DisplayList) -> Vec<Rect> {
    display_list
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

pub(crate) fn checker() -> Arc<RgbaImage> {
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

// --- R94: origin/clip/attachment, multiple layers and space/round -------------

pub(crate) use megi_css::{
    BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin,
};

pub(crate) fn layer(
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

pub(crate) fn styled_layers(layers: Vec<BackgroundLayer>) -> ComputedStyle {
    let mut style = block_style();
    style.background_layers = layers;
    style.background_geometry_explicit = true;
    style
}
