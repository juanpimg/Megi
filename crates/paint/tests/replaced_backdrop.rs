//! `object-fit`/`object-position` and `backdrop-filter` painting (R149).

use std::sync::Arc;

use css::{BackgroundPosition, Color, ComputedStyle, Display, FilterFunction, Length, ObjectFit};
use dom::{Document, Namespace};
use image::RgbaImage;
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, DisplayItem, ImageStore, Pixmap,
    build_display_list_with_images_and_backgrounds, rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 80.0,
    height: 60.0,
};

fn base_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn checker_image() -> Arc<RgbaImage> {
    // 2x2 quadrants: red, green / blue, magenta.
    Arc::new(
        RgbaImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
            ],
        )
        .expect("RGBA image"),
    )
}

fn dims(rect: Rect) -> Dimensions {
    Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    }
}

/// Builds `html > children` where each child is either a normal block or a
/// replaced box (when `image` is Some) and paints it over a white canvas.
struct Child {
    style: ComputedStyle,
    rect: Rect,
    image: Option<Arc<RgbaImage>>,
}

fn render(children: Vec<Child>) -> (Vec<DisplayItem>, Pixmap) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut root_style = base_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(html), Some(root_style));
    root.dimensions = dims(VIEWPORT);
    let mut images: ImageStore = ImageStore::new();
    for child in children {
        let node = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, node);
        let box_type = if child.image.is_some() {
            BoxType::Replaced(node)
        } else {
            BoxType::BlockNode(node)
        };
        if let Some(image) = child.image {
            images.insert(node, image);
        }
        let mut layout_box = LayoutBox::new(box_type, Some(child.style));
        layout_box.dimensions = dims(child.rect);
        layout_box.baseline = child.rect.height;
        root.children.push(layout_box);
    }
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &images,
        &BackgroundImages::new(),
    );
    let pixmap = rasterize_display_list(&list, 80, 60);
    (list.items, pixmap)
}

fn object_style(fit: ObjectFit, position: &str) -> ComputedStyle {
    let mut style = base_style();
    style.object_fit = fit;
    style.object_position = BackgroundPosition::parse(position, 16.0, 16.0).expect("position");
    style
}

fn replaced(style: ComputedStyle, rect: Rect) -> Child {
    Child {
        style,
        rect,
        image: Some(checker_image()),
    }
}

fn redish(color: Color) -> bool {
    color.r > 150 && color.g < 100 && color.b < 100
}

#[test]
fn test_object_fit_fill_keeps_legacy_display_list() {
    let items = render(vec![replaced(
        object_style(ObjectFit::Fill, "center"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )])
    .0;
    let image_items: Vec<Rect> = items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(image_items, vec![Rect::new(10.0, 10.0, 40.0, 20.0)]);
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, DisplayItem::PushClip { .. })),
        "fill needs no clip item"
    );
}

#[test]
fn test_object_fit_contain_letterboxes() {
    let (_, pixmap) = render(vec![replaced(
        object_style(ObjectFit::Contain, "center"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )]);
    // 2x2 image in a 40x20 box: contain => 20x20 centred at x=20.
    assert_eq!(pixmap.pixel(15, 20), Color::WHITE, "left letterbox bar");
    assert!(
        redish(pixmap.pixel(25, 15)),
        "top-left quadrant red: {:?}",
        pixmap.pixel(25, 15)
    );
    assert!(pixmap.pixel(35, 15).g > 150, "top-right quadrant green");
    assert!(pixmap.pixel(25, 25).b > 150, "bottom-left quadrant blue");
    assert!(
        pixmap.pixel(35, 25).r > 150 && pixmap.pixel(35, 25).b > 150,
        "magenta"
    );
}

#[test]
fn test_object_fit_cover_crops() {
    let (_, pixmap) = render(vec![replaced(
        object_style(ObjectFit::Cover, "center"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )]);
    // 2x2 image scaled to 40x40 and centred: the visible 20px band shows the
    // middle of the image (top half red/green, bottom half blue/magenta).
    let top_left = pixmap.pixel(15, 12);
    let top_right = pixmap.pixel(45, 12);
    let bottom_left = pixmap.pixel(15, 28);
    assert!(
        top_left.r > 120 || top_left.g > 120,
        "top band: {top_left:?}"
    );
    assert!(top_right.g > 120, "top-right is green-ish: {top_right:?}");
    assert!(
        bottom_left.b > 120,
        "bottom band is blue-ish: {bottom_left:?}"
    );
    assert_eq!(
        pixmap.pixel(5, 20),
        Color::WHITE,
        "cover is clipped to the box"
    );
}

#[test]
fn test_object_fit_none_and_positions() {
    let (_, pixmap) = render(vec![
        replaced(
            object_style(ObjectFit::None, "top left"),
            Rect::new(10.0, 10.0, 20.0, 20.0),
        ),
        replaced(
            object_style(ObjectFit::None, "right bottom"),
            Rect::new(40.0, 10.0, 20.0, 20.0),
        ),
    ]);
    // top-left: 2x2 at the content origin -> red at (10,10).
    assert!(redish(pixmap.pixel(10, 10)));
    // right-bottom: 2x2 at content bottom-right corner (58,28).
    assert_eq!(pixmap.pixel(50, 20), Color::WHITE);
    assert!(
        pixmap.pixel(59, 29).b > 150,
        "magenta at bottom-right: {:?}",
        pixmap.pixel(59, 29)
    );
}

#[test]
fn test_object_fit_scale_down() {
    // Small image (2x2) in a large box behaves like `none`.
    let (_, pixmap) = render(vec![replaced(
        object_style(ObjectFit::ScaleDown, "center"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )]);
    assert_eq!(pixmap.pixel(15, 20), Color::WHITE);
    assert!(
        redish(pixmap.pixel(29, 19)),
        "centred 2x2 red: {:?}",
        pixmap.pixel(29, 19)
    );
    // Large image (40x40) in a 40x20 box scales down like `contain` (20x20).
    let large = Arc::new(RgbaImage::new(40, 40, [0u8, 0, 255, 255].repeat(1600)).expect("RGBA"));
    let (_, pixmap) = render(vec![Child {
        style: object_style(ObjectFit::ScaleDown, "center"),
        rect: Rect::new(10.0, 10.0, 40.0, 20.0),
        image: Some(Arc::clone(&large)),
    }]);
    assert_eq!(
        pixmap.pixel(15, 20),
        Color::WHITE,
        "letterbox after scale-down"
    );
    assert_eq!(pixmap.pixel(25, 15), Color::BLUE);
}

#[test]
fn test_object_position_percent_and_pixels() {
    let (_, pixmap) = render(vec![
        replaced(
            object_style(ObjectFit::None, "100% 0%"),
            Rect::new(10.0, 10.0, 20.0, 20.0),
        ),
        replaced(
            object_style(ObjectFit::None, "10px 5px"),
            Rect::new(40.0, 10.0, 20.0, 20.0),
        ),
    ]);
    // 100% 0% => top-right corner of the content box (28,10).
    assert!(
        pixmap.pixel(29, 10).g > 120,
        "top-right image pixel: {:?}",
        pixmap.pixel(29, 10)
    );
    // 10px 5px offset from the content box origin.
    assert!(
        redish(pixmap.pixel(50, 15)),
        "offset image at (50,15): {:?}",
        pixmap.pixel(50, 15)
    );
}

fn backdrop_style(filters: Vec<FilterFunction>) -> ComputedStyle {
    let mut style = base_style();
    style.backdrop_filter = filters;
    style.background_color = Color::TRANSPARENT;
    style
}

fn solid(color: Color, rect: Rect) -> Child {
    let mut style = base_style();
    style.background_color = color;
    Child {
        style,
        rect,
        image: None,
    }
}

#[test]
fn test_backdrop_filter_brightness_darkens_backdrop() {
    let (items, pixmap) = render(vec![
        solid(Color::BLUE, Rect::new(10.0, 10.0, 20.0, 20.0)),
        Child {
            style: backdrop_style(vec![FilterFunction::Brightness(0.5)]),
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            image: None,
        },
    ]);
    assert!(
        items
            .iter()
            .any(|item| matches!(item, DisplayItem::BackdropFilter { .. }))
    );
    let pixel = pixmap.pixel(20, 20);
    assert!(pixel.b > 100 && pixel.b < 160, "blue darkens: {pixel:?}");
    assert_eq!(
        pixmap.pixel(5, 5),
        Color::WHITE,
        "outside the box untouched"
    );
}

#[test]
fn test_backdrop_filter_blur_smears_backdrop() {
    let (_, plain) = render(vec![
        solid(Color::RED, Rect::new(10.0, 10.0, 20.0, 20.0)),
        solid(Color::BLUE, Rect::new(30.0, 10.0, 20.0, 20.0)),
    ]);
    let (_, blurred) = render(vec![
        solid(Color::RED, Rect::new(10.0, 10.0, 20.0, 20.0)),
        solid(Color::BLUE, Rect::new(30.0, 10.0, 20.0, 20.0)),
        Child {
            style: backdrop_style(vec![FilterFunction::Blur(3.0)]),
            rect: Rect::new(10.0, 10.0, 40.0, 20.0),
            image: None,
        },
    ]);
    assert_ne!(
        plain.pixel(29, 20),
        blurred.pixel(29, 20),
        "blur changes the seam"
    );
    assert!(
        blurred.pixel(29, 20).b > plain.pixel(29, 20).b,
        "red side picks blue: plain={:?} blurred={:?}",
        plain.pixel(29, 20),
        blurred.pixel(29, 20)
    );
}

#[test]
fn test_backdrop_filter_caps_and_determinism() {
    let many: Vec<FilterFunction> = (0..20)
        .map(|index| FilterFunction::Brightness(0.9 + index as f32 * 0.01))
        .collect();
    let render_once = || {
        render(vec![
            solid(Color::BLUE, Rect::new(10.0, 10.0, 20.0, 20.0)),
            Child {
                style: backdrop_style(many.clone()),
                rect: Rect::new(10.0, 10.0, 20.0, 20.0),
                image: None,
            },
        ])
        .1
        .to_rgba_bytes()
    };
    assert_eq!(render_once(), render_once());
    let (_, pixmap) = render(vec![
        solid(Color::BLUE, Rect::new(10.0, 10.0, 20.0, 20.0)),
        Child {
            style: backdrop_style(vec![FilterFunction::Blur(10_000.0)]),
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            image: None,
        },
    ]);
    assert_eq!(pixmap.pixel(5, 5), Color::WHITE);
}

#[test]
fn test_no_backdrop_filter_keeps_legacy_display_list() {
    let (items, pixmap) = render(vec![solid(Color::BLUE, Rect::new(10.0, 10.0, 20.0, 20.0))]);
    assert!(
        items
            .iter()
            .all(|item| !matches!(item, DisplayItem::BackdropFilter { .. })),
        "no backdrop item without the property"
    );
    assert_eq!(pixmap.pixel(20, 20), Color::BLUE);
}

#[test]
fn test_object_fit_from_css_resolution() {
    use dom::Attribute;
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let node = document.create_element(
        Namespace::Html,
        "img",
        vec![Attribute::new(
            "style",
            "width: 40px; height: 20px; object-fit: contain; object-position: left center; \
             backdrop-filter: brightness(0.5);",
        )],
    );
    document.append_child(document.root(), html);
    document.append_child(html, body);
    document.append_child(body, node);
    let styles = css::resolve_document_styles(&document, None);
    let style = styles.get(&node).cloned().expect("style");
    assert_eq!(style.object_fit, ObjectFit::Contain);
    assert_eq!(style.object_position.x, Length::Percent(0.0));
    assert_eq!(style.backdrop_filter.len(), 1);
}

#[test]
fn test_object_fit_contain_with_corner_position() {
    let mut style = object_style(ObjectFit::Contain, "0% 100%");
    style.background_color = Color::WHITE;
    let (_, pixmap) = render(vec![replaced(style, Rect::new(10.0, 10.0, 40.0, 20.0))]);
    // 20x20 image aligned to the bottom-left of the 40x20 content box.
    assert!(
        redish(pixmap.pixel(11, 11)),
        "top-left content pixel: {:?}",
        pixmap.pixel(11, 11)
    );
    assert!(
        pixmap.pixel(11, 29).b > 120,
        "bottom band blue: {:?}",
        pixmap.pixel(11, 29)
    );
    assert_eq!(
        pixmap.pixel(35, 15),
        Color::WHITE,
        "right side is the letterbox"
    );
}

#[test]
fn test_backdrop_filter_grayscale() {
    let (_, pixmap) = render(vec![
        solid(Color::RED, Rect::new(10.0, 10.0, 20.0, 20.0)),
        Child {
            style: backdrop_style(vec![
                FilterFunction::Grayscale(1.0),
                FilterFunction::Opacity(1.0),
            ]),
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            image: None,
        },
    ]);
    let pixel = pixmap.pixel(20, 20);
    assert!(
        pixel.r.abs_diff(pixel.g) < 6 && pixel.g.abs_diff(pixel.b) < 6,
        "backdrop becomes gray: {pixel:?}"
    );
    assert!(pixel.r > 30 && pixel.r < 90, "red luma retained: {pixel:?}");
}

#[test]
fn test_object_fit_cover_clips_to_content_box() {
    let (items, _pixmap) = render(vec![replaced(
        object_style(ObjectFit::Cover, "center"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )]);
    assert!(
        items.iter().any(|item| matches!(
            item,
            DisplayItem::PushClip { rect } if *rect == Rect::new(10.0, 10.0, 40.0, 20.0)
        )),
        "cover clips the overflowing image to the content box"
    );
}

#[test]
fn test_object_fit_cover_with_position_top_left() {
    let (_, pixmap) = render(vec![replaced(
        object_style(ObjectFit::Cover, "0% 0%"),
        Rect::new(10.0, 10.0, 40.0, 20.0),
    )]);
    // Cover with top-left alignment shows the image top-left quadrant at the
    // content origin (red) and the clipped right/bottom edges.
    assert!(
        redish(pixmap.pixel(11, 11)),
        "top-left red: {:?}",
        pixmap.pixel(11, 11)
    );
    assert!(
        pixmap.pixel(45, 12).g > 120,
        "top-right green: {:?}",
        pixmap.pixel(45, 12)
    );
}
