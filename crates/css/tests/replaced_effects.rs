//! `aspect-ratio`, `object-fit`/`object-position` and `backdrop-filter` parsing (R149).

use css::{BackgroundPosition, Length, ObjectFit, resolve_document_styles};
use dom::{Attribute, Document, Namespace};

fn computed_style(inline: &str) -> css::ComputedStyle {
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new("style", inline)],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let resolved = resolve_document_styles(&document, None);
    resolved.get(&element).cloned().expect("computed style")
}

fn style_pair(parent: &str, child: &str) -> (css::ComputedStyle, css::ComputedStyle) {
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let parent_node = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new("style", parent)],
    );
    let child_node =
        document.create_element(Namespace::Html, "p", vec![Attribute::new("style", child)]);
    document.append_child(document.root(), body);
    document.append_child(body, parent_node);
    document.append_child(parent_node, child_node);
    let styles = resolve_document_styles(&document, None);
    (
        styles.get(&parent_node).cloned().expect("parent"),
        styles.get(&child_node).cloned().expect("child"),
    )
}

#[test]
fn test_defaults() {
    let style = css::ComputedStyle::initial();
    assert_eq!(style.aspect_ratio, None);
    assert_eq!(style.object_fit, ObjectFit::Fill);
    assert_eq!(style.object_position.x, Length::Percent(50.0));
    assert_eq!(style.object_position.y, Length::Percent(50.0));
    assert!(style.backdrop_filter.is_empty());
}

#[test]
fn test_aspect_ratio_ratios() {
    for (value, expected) in [
        ("16 / 9", 16.0 / 9.0),
        ("4/3", 4.0 / 3.0),
        ("1.5", 1.5),
        ("1 / 2", 0.5),
    ] {
        let style = computed_style(&format!("aspect-ratio: {value};"));
        let aspect = style.aspect_ratio.expect("ratio parsed");
        assert!(!aspect.auto);
        assert!((aspect.ratio - expected).abs() < 1e-5, "{value}");
    }
}

#[test]
fn test_aspect_ratio_auto_forms() {
    assert_eq!(computed_style("aspect-ratio: auto;").aspect_ratio, None);
    let auto_ratio = computed_style("aspect-ratio: auto 16 / 9;")
        .aspect_ratio
        .expect("auto ratio");
    assert!(auto_ratio.auto);
    assert!((auto_ratio.ratio - 16.0 / 9.0).abs() < 1e-5);
}

#[test]
fn test_aspect_ratio_invalid_values() {
    for value in [
        "0 / 0", "-1 / 2", "16 / 0", "auto / 2", "wide", "16 9", "1/2/3",
    ] {
        let style = computed_style(&format!("aspect-ratio: {value};"));
        assert_eq!(style.aspect_ratio, None, "{value}");
    }
}

#[test]
fn test_object_fit_keywords() {
    for (value, expected) in [
        ("fill", ObjectFit::Fill),
        ("contain", ObjectFit::Contain),
        ("cover", ObjectFit::Cover),
        ("none", ObjectFit::None),
        ("scale-down", ObjectFit::ScaleDown),
        ("CONTAIN", ObjectFit::Contain),
        ("stretch", ObjectFit::Fill), // invalid falls back to the initial value
    ] {
        assert_eq!(
            computed_style(&format!("object-fit: {value};")).object_fit,
            expected
        );
    }
}

#[test]
fn test_object_position_forms() {
    let center = computed_style("object-position: center;").object_position;
    assert_eq!(center.x, Length::Percent(50.0));
    assert_eq!(center.y, Length::Percent(50.0));
    let top_left = computed_style("object-position: top left;").object_position;
    assert_eq!(top_left.x, Length::Percent(0.0));
    assert_eq!(top_left.y, Length::Percent(0.0));
    let mixed = computed_style("object-position: 10px 75%;").object_position;
    assert_eq!(mixed.x, Length::Px(10.0));
    assert_eq!(mixed.y, Length::Percent(75.0));
    let right_bottom = computed_style("object-position: right bottom;").object_position;
    assert_eq!(right_bottom.x, Length::Percent(100.0));
    assert_eq!(right_bottom.y, Length::Percent(100.0));
}

#[test]
fn test_backdrop_filter_parsing() {
    let style = computed_style("backdrop-filter: blur(4px) brightness(0.5) grayscale(1);");
    assert_eq!(style.backdrop_filter.len(), 3);
    let prefixed = computed_style("-webkit-backdrop-filter: invert(1);");
    assert_eq!(prefixed.backdrop_filter.len(), 1);
    // Unprefixed wins when both are authored.
    let both = computed_style("-webkit-backdrop-filter: invert(1); backdrop-filter: sepia(50%);");
    assert_eq!(both.backdrop_filter.len(), 1);
    // Invalid function lists fall back to none.
    assert!(
        computed_style("backdrop-filter: nope(1);")
            .backdrop_filter
            .is_empty()
    );
    assert!(
        computed_style("backdrop-filter: none;")
            .backdrop_filter
            .is_empty()
    );
}

#[test]
fn test_backdrop_filter_cap() {
    let value = (0..20)
        .map(|index| format!("brightness({})", 1.0 + index as f32 * 0.05))
        .collect::<Vec<_>>()
        .join(" ");
    let style = computed_style(&format!("backdrop-filter: {value};"));
    assert_eq!(style.backdrop_filter.len(), css::MAX_FILTER_FUNCTIONS);
    assert!(style.backdrop_filter.len() <= 16);
}

#[test]
fn test_new_properties_are_not_inherited() {
    let (parent, child) = style_pair(
        "aspect-ratio: 2 / 1; object-fit: cover; object-position: top left; \
         backdrop-filter: blur(2px);",
        "",
    );
    assert!(parent.aspect_ratio.is_some());
    assert_eq!(parent.object_fit, ObjectFit::Cover);
    assert!(child.aspect_ratio.is_none());
    assert_eq!(child.object_fit, ObjectFit::Fill);
    assert_eq!(
        child.object_position,
        BackgroundPosition {
            x: Length::Percent(50.0),
            y: Length::Percent(50.0)
        }
    );
    assert!(child.backdrop_filter.is_empty());
}

#[test]
fn test_aspect_ratio_whitespace_and_decimal_forms() {
    for (value, expected) in [
        ("16/9", 16.0 / 9.0),
        ("16 / 9", 16.0 / 9.0),
        ("1.777", 1.777),
        ("0.5 / 2", 0.25),
    ] {
        let style = computed_style(&format!("aspect-ratio: {value};"));
        assert!(
            (style.aspect_ratio.expect("ratio").ratio - expected).abs() < 1e-4,
            "{value}"
        );
    }
}

#[test]
fn test_object_position_invalid_falls_back_to_center() {
    let style = computed_style("object-position: nope;");
    assert_eq!(style.object_position.x, Length::Percent(50.0));
    assert_eq!(style.object_position.y, Length::Percent(50.0));
}

#[test]
fn test_object_fit_invalid_falls_back_to_fill() {
    assert_eq!(
        computed_style("object-fit: 42px;").object_fit,
        ObjectFit::Fill
    );
}

#[test]
fn test_backdrop_filter_order_is_preserved() {
    let style = computed_style("backdrop-filter: grayscale(1) blur(2px) brightness(0.5);");
    assert!(matches!(
        style.backdrop_filter[0],
        css::FilterFunction::Grayscale(1.0)
    ));
    assert!(matches!(
        style.backdrop_filter[1],
        css::FilterFunction::Blur(2.0)
    ));
    assert!(matches!(
        style.backdrop_filter[2],
        css::FilterFunction::Brightness(0.5)
    ));
}

#[test]
fn test_prefixed_backdrop_filter_used_when_unprefixed_invalid() {
    // The invalid unprefixed declaration is ignored by CSS and the prefixed
    // alias applies (unprefixed only wins when it parses).
    let style = computed_style("-webkit-backdrop-filter: sepia(1); backdrop-filter: nope(1);");
    assert_eq!(style.backdrop_filter.len(), 1);
    assert!(matches!(
        style.backdrop_filter[0],
        css::FilterFunction::Sepia(1.0)
    ));
}

#[test]
fn test_aspect_ratio_auto_only_keeps_intrinsic_behaviour() {
    assert_eq!(computed_style("aspect-ratio: auto;").aspect_ratio, None);
    let ratio = computed_style("aspect-ratio: auto 2/1;")
        .aspect_ratio
        .expect("ratio");
    assert!(ratio.auto);
    assert!((ratio.ratio - 2.0).abs() < 1e-5);
}
