pub(crate) use std::sync::Arc;

pub(crate) use megi_core::{Attribute, Document, Namespace};
pub(crate) use megi_css::{
    BackgroundClip, BackgroundLayer, BorderRadii, Color, ComputedStyle, Display, Gradient,
    GradientKind, GradientStop, Length, LinearDirection, LinearGradient, RadialExtent,
    RadialGradient, RadialShape, Radius, resolve_document_styles,
};
pub(crate) use megi_layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
pub(crate) use megi_paint::{
    BackgroundImages, ImageStore, Pixmap, build_display_list_with_images_and_backgrounds,
    rasterize_display_list,
};

pub(crate) const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 60.0,
    height: 40.0,
};

pub(crate) fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

pub(crate) fn linear_gradient(
    direction: LinearDirection,
    stops: Vec<(&str, Option<f32>)>,
) -> Gradient {
    let stops = stops
        .into_iter()
        .map(|(hex, position)| GradientStop {
            color: Color::parse(hex).expect("color"),
            position: position.map(Length::Percent),
        })
        .collect();
    Gradient {
        kind: GradientKind::Linear(LinearGradient { direction }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: megi_css::InterpolationSpace::Srgb,
    }
}

pub(crate) fn gradient_style(gradient: Gradient) -> ComputedStyle {
    let mut style = block_style();
    style.background_layers = vec![BackgroundLayer {
        gradient: Some(Arc::new(gradient)),
        ..BackgroundLayer::default()
    }];
    style.background_geometry_explicit = true;
    style
}

pub(crate) fn render(style: ComputedStyle, rect: Rect, children: Vec<LayoutBox>) -> Pixmap {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    let node = document.create_element(Namespace::Html, "div", vec![]);
    document.append_child(document.root(), html);
    document.append_child(html, node);
    let mut root_style = block_style();
    root_style.background_color = Color::WHITE;
    let mut root = LayoutBox::new(BoxType::BlockNode(node), Some(root_style));
    root.dimensions.content = VIEWPORT;
    let mut child = LayoutBox::new(BoxType::BlockNode(node), Some(style));
    child.dimensions.content = rect;
    child.children = children;
    root.children.push(child);
    let list = build_display_list_with_images_and_backgrounds(
        &root,
        VIEWPORT,
        &ImageStore::new(),
        &BackgroundImages::new(),
    );
    rasterize_display_list(&list, 60, 40)
}

pub(crate) fn transparent_text(text: &str, rect: Rect, baseline: f32) -> LayoutBox {
    let mut style = block_style();
    style.display = Display::Inline;
    style.color = Color::TRANSPARENT;
    style.font_size = 32.0;
    style.font_family = "DejaVu Sans".to_string();
    let mut text_box = LayoutBox::new(BoxType::TextNode(text.to_string()), Some(style));
    text_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    text_box.baseline = baseline;
    text_box
}

pub(crate) fn conic_gradient(from_angle: f32, stops: Vec<(&str, Option<f32>)>) -> Gradient {
    let stops = stops
        .into_iter()
        .map(|(hex, position)| GradientStop {
            color: Color::parse(hex).expect("color"),
            position: position.map(Length::Percent),
        })
        .collect();
    Gradient {
        kind: GradientKind::Conic(megi_css::ConicGradient {
            from_angle,
            position: megi_css::BackgroundPosition::parse("center", 16.0, 16.0).expect("center"),
        }),
        repeating: false,
        stops,
        hints: Vec::new(),
        interpolation: megi_css::InterpolationSpace::Srgb,
    }
}
