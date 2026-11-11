//! Modern CSS colors reaching paint: background, gradients and color-mix (R159).

use css::{Color, ComputedStyle, Display};
use dom::{Document, Namespace};
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    BackgroundImages, ImageStore, build_display_list_with_images_and_backgrounds,
    rasterize_display_list,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 40.0,
    height: 20.0,
};

fn render(style: ComputedStyle) -> paint::Pixmap {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut root_style = ComputedStyle::initial();
    root_style.display = Display::Block;
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(html), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = LayoutBox::new(BoxType::BlockNode(html), Some(style));
    child.dimensions = Dimensions {
        content: Rect::new(0.0, 0.0, 20.0, 10.0),
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    root.children.push(child);
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    rasterize_display_list(&list, 40, 20)
}

#[test]
fn test_modern_background_color_paints_srgb() {
    for (value, expected) in [
        ("lab(50 0 0)", (119u8, 119u8, 119u8)),
        ("color(display-p3 1 0 0)", (255, 0, 0)),
        ("oklab(0.5 0 0)", (99, 99, 99)),
        ("hwb(0 0% 0%)", (255, 0, 0)),
    ] {
        let mut style = ComputedStyle::initial();
        style.display = Display::Block;
        style.background_color = Color::parse(value).expect("modern color");
        let pixmap = render(style);
        let pixel = pixmap.pixel(5, 5);
        let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
        assert!(
            near(pixel.r, expected.0) && near(pixel.g, expected.1) && near(pixel.b, expected.2),
            "{value}: got {pixel:?}"
        );
    }
}

#[test]
fn test_modern_gradient_stops_paint() {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.background_layers = vec![css::BackgroundLayer {
        gradient: Some(std::sync::Arc::new(css::Gradient {
            kind: css::GradientKind::Linear(css::LinearGradient {
                direction: css::LinearDirection::ToRight,
            }),
            repeating: false,
            stops: vec![
                css::GradientStop {
                    color: Color::parse("color(srgb 1 0 0)").expect("red"),
                    position: None,
                },
                css::GradientStop {
                    color: Color::parse("color(srgb 0 0 1)").expect("blue"),
                    position: None,
                },
            ],
            hints: Vec::new(),
            interpolation: css::InterpolationSpace::Srgb,
        })),
        ..css::BackgroundLayer::default()
    }];
    style.background_geometry_explicit = true;
    let pixmap = render(style);
    assert!(
        pixmap.pixel(0, 5).r > 200,
        "left red: {:?}",
        pixmap.pixel(0, 5)
    );
    assert!(
        pixmap.pixel(19, 5).b > 200,
        "right blue: {:?}",
        pixmap.pixel(19, 5)
    );
}

#[test]
fn test_color_mix_background_paints_mixed_color() {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.background_color = Color::parse("color-mix(in srgb, red, blue)").expect("color-mix");
    let pixmap = render(style);
    let pixel = pixmap.pixel(5, 5);
    assert!(
        pixel.r.abs_diff(128) <= 2 && pixel.g <= 2 && pixel.b.abs_diff(128) <= 2,
        "srgb mix: {pixel:?}"
    );
}
