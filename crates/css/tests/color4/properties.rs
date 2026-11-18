//! Modern colors inside computed CSS properties and shadows.

use crate::common::assert_color;

#[test]
fn test_modern_colors_in_properties() {
    use megi_core::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "background-color: lab(50 0 0); color: color(display-p3 1 0 0); \
             border: 2px solid oklab(0.5 0 0); \
             background-image: linear-gradient(color(srgb 1 0 0), color(srgb 0 0 1)); \
             text-shadow: 1px 1px 0 color(srgb 0 1 0);",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = megi_css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    assert_color(
        Some(style.background_color),
        (119, 119, 119),
        2,
        "background lab",
    );
    assert_color(Some(style.color), (255, 0, 0), 1, "color p3");
    assert_color(
        Some(style.border_color.top),
        (99, 99, 99),
        3,
        "border oklab",
    );
    let gradient = style.background_layers[0]
        .gradient
        .as_ref()
        .expect("gradient");
    assert_color(Some(gradient.stops[0].color), (255, 0, 0), 1, "stop 1");
    assert_color(Some(gradient.stops[1].color), (0, 0, 255), 1, "stop 2");
    assert_color(
        Some(style.text_shadows[0].color),
        (0, 255, 0),
        1,
        "text shadow",
    );
}

#[test]
fn test_modern_colors_in_filter_and_drop_shadow() {
    use megi_core::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "filter: drop-shadow(1px 1px 0 color(srgb 0 1 0));",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = megi_css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    match style.filter.first().expect("filter function") {
        megi_css::FilterFunction::DropShadow { color, .. } => {
            assert_color(Some(*color), (0, 255, 0), 1, "drop shadow color");
        }
        other => panic!("expected drop-shadow, got {other:?}"),
    }
}

#[test]
fn test_modern_colors_in_conic_gradient_stops() {
    use megi_core::{Attribute, Document, Namespace};
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "background-image: conic-gradient(color-mix(in srgb, red, blue) 0deg, \
             color(srgb 1 1 1) 180deg);",
        )],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let styles = megi_css::resolve_document_styles(&document, None);
    let style = styles.get(&element).cloned().expect("style");
    let gradient = style.background_layers[0]
        .gradient
        .as_ref()
        .expect("gradient");
    assert_eq!(gradient.stops.len(), 2);
    assert_color(
        Some(gradient.stops[0].color),
        (128, 0, 128),
        1,
        "conic stop 1",
    );
    assert_color(
        Some(gradient.stops[1].color),
        (255, 255, 255),
        1,
        "conic stop 2",
    );
}

#[test]
fn shadow_list_rejects_entire_declaration_when_any_layer_is_invalid() {
    use megi_core::{Attribute, Document, Namespace};

    let check = |style_attr: &str| -> megi_css::ComputedStyle {
        let mut document = Document::new();
        let body = document.create_element(Namespace::Html, "body", vec![]);
        let element = document.create_element(
            Namespace::Html,
            "div",
            vec![Attribute::new("style", style_attr)],
        );
        document.append_child(document.root(), body);
        document.append_child(body, element);
        let styles = megi_css::resolve_document_styles(&document, None);
        styles.get(&element).cloned().expect("style")
    };

    let valid = check(
        "box-shadow: 2px 4px 6px red, inset 1px 2px blue; text-shadow: 1px 2px red, 3px 4px 5px blue;",
    );
    assert_eq!(valid.box_shadows.len(), 2);
    assert_eq!(valid.text_shadows.len(), 2);

    for invalid_box in [
        "box-shadow: 2px 4px red, none;",
        "box-shadow: 2px 4px red, 5px;",
        "box-shadow: 2px 4px red, 1px 2px -3px blue;",
        "box-shadow: 2px 4px red, 1px 2px unknowntoken;",
        "box-shadow: 2px 4px red, ;",
    ] {
        let style = check(invalid_box);
        assert!(
            style.box_shadows.is_empty(),
            "expected empty box_shadows for `{invalid_box}`, got {:?}",
            style.box_shadows
        );
    }

    for invalid_text in [
        "text-shadow: 1px 2px red, none;",
        "text-shadow: 1px 2px red, 3px;",
        "text-shadow: 1px 2px red, 1px 2px -1px blue;",
        "text-shadow: 1px 2px red, inset 1px 2px blue;",
    ] {
        let style = check(invalid_text);
        assert!(
            style.text_shadows.is_empty(),
            "expected empty text_shadows for `{invalid_text}`, got {:?}",
            style.text_shadows
        );
    }
}
