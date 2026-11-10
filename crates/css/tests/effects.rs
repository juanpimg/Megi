//! `filter`, `mix-blend-mode` and `image-set()` parsing (R137).

use css::{FilterFunction, MAX_FILTER_FUNCTIONS, MixBlendMode, resolve_document_styles};
use dom::{Attribute, Document, Namespace};

fn computed_style(css_text: &str, inline: &str) -> css::ComputedStyle {
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let element = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new("style", inline)],
    );
    document.append_child(document.root(), body);
    document.append_child(body, element);
    let resolved = resolve_document_styles(&document, Some(css_text));
    resolved.get(&element).cloned().expect("computed style")
}

#[test]
fn test_filter_defaults_to_none() {
    let style = css::ComputedStyle::initial();
    assert!(style.filter.is_empty());
    assert_eq!(style.mix_blend_mode, MixBlendMode::Normal);
    assert!(computed_style("", "filter: none;").filter.is_empty());
}

#[test]
fn test_filter_numeric_functions() {
    let style = computed_style(
        "",
        "filter: brightness(1.5) contrast(50%) grayscale(1) invert(0.25) opacity(30%) sepia(0.5) saturate(2);",
    );
    assert_eq!(
        style.filter,
        vec![
            FilterFunction::Brightness(1.5),
            FilterFunction::Contrast(0.5),
            FilterFunction::Grayscale(1.0),
            FilterFunction::Invert(0.25),
            FilterFunction::Opacity(0.3),
            FilterFunction::Sepia(0.5),
            FilterFunction::Saturate(2.0),
        ]
    );
}

#[test]
fn test_filter_blur_and_hue_rotate() {
    let style = computed_style("", "filter: blur(4px) hue-rotate(90deg);");
    assert_eq!(
        style.filter,
        vec![FilterFunction::Blur(4.0), FilterFunction::HueRotate(90.0)]
    );
    let turn = computed_style("", "filter: hue-rotate(0.5turn);");
    assert_eq!(turn.filter, vec![FilterFunction::HueRotate(180.0)]);
    let em = computed_style("", "font-size: 20px; filter: blur(0.5em);");
    assert_eq!(em.filter, vec![FilterFunction::Blur(10.0)]);
}

#[test]
fn test_filter_drop_shadow() {
    let style = computed_style("", "filter: drop-shadow(2px 3px 4px rgba(0,0,0,0.5));");
    assert_eq!(
        style.filter,
        vec![FilterFunction::DropShadow {
            offset_x: 2.0,
            offset_y: 3.0,
            blur: 4.0,
            color: css::Color::new(0, 0, 0, 128),
        }]
    );
    let current = computed_style(
        "div { color: #336699; }",
        "filter: drop-shadow(1px 1px currentColor);",
    );
    assert_eq!(
        current.filter,
        vec![FilterFunction::DropShadow {
            offset_x: 1.0,
            offset_y: 1.0,
            blur: 0.0,
            color: css::Color::new(0x33, 0x66, 0x99, 255),
        }]
    );
}

#[test]
fn test_webkit_filter_alias() {
    let style = computed_style("", "-webkit-filter: grayscale(100%);");
    assert_eq!(style.filter, vec![FilterFunction::Grayscale(1.0)]);
    // The unprefixed property wins when both are authored.
    let both = computed_style("", "-webkit-filter: grayscale(1); filter: brightness(2);");
    assert_eq!(both.filter, vec![FilterFunction::Brightness(2.0)]);
}

#[test]
fn test_invalid_filter_drops_declaration() {
    for invalid in [
        "filter: blur(-2px);",
        "filter: hue-rotate(red);",
        "filter: unknown(1);",
        "filter: drop-shadow(1px);",
        "filter: brightness(1.2) nope(3);",
    ] {
        let style = computed_style("", invalid);
        assert!(
            style.filter.is_empty(),
            "expected filter:none for {invalid}"
        );
    }
}

#[test]
fn test_filter_function_cap() {
    let value = (0..20)
        .map(|index| format!("brightness({})", 1.0 + index as f32 * 0.1))
        .collect::<Vec<_>>()
        .join(" ");
    let style = computed_style("", &format!("filter: {value};"));
    assert_eq!(style.filter.len(), MAX_FILTER_FUNCTIONS);
    assert!(style.filter.len() <= 16);
}

#[test]
fn test_mix_blend_mode_values() {
    for (value, expected) in [
        ("normal", MixBlendMode::Normal),
        ("multiply", MixBlendMode::Multiply),
        ("screen", MixBlendMode::Screen),
        ("overlay", MixBlendMode::Overlay),
        ("darken", MixBlendMode::Darken),
        ("lighten", MixBlendMode::Lighten),
    ] {
        let style = computed_style("", &format!("mix-blend-mode: {value};"));
        assert_eq!(style.mix_blend_mode, expected, "{value}");
    }
    // Unknown values fall back to normal; keywords are case-insensitive.
    assert_eq!(
        computed_style("", "mix-blend-mode: color-dodge;").mix_blend_mode,
        MixBlendMode::Normal
    );
    assert_eq!(
        computed_style("", "mix-blend-mode: SCREEN;").mix_blend_mode,
        MixBlendMode::Screen
    );
}

#[test]
fn test_effects_are_not_inherited() {
    let mut document = Document::new();
    let body = document.create_element(Namespace::Html, "body", vec![]);
    let parent = document.create_element(
        Namespace::Html,
        "div",
        vec![Attribute::new(
            "style",
            "filter: grayscale(1); mix-blend-mode: multiply;",
        )],
    );
    let child = document.create_element(Namespace::Html, "p", vec![]);
    document.append_child(document.root(), body);
    document.append_child(body, parent);
    document.append_child(parent, child);
    let styles = resolve_document_styles(&document, None);
    let child_style = styles.get(&child).expect("child style");
    assert!(child_style.filter.is_empty());
    assert_eq!(child_style.mix_blend_mode, MixBlendMode::Normal);
    assert_eq!(styles.get(&parent).expect("parent").filter.len(), 1);
}

#[test]
fn test_image_set_selects_smallest_resolution() {
    let style = computed_style(
        "",
        "background-image: image-set(url(a.png) 2x, url(b.png) 1x);",
    );
    assert_eq!(style.background_image, Some("b.png".to_string()));
    let only_2x = computed_style("", "background-image: image-set(url(c.png) 2x);");
    assert_eq!(only_2x.background_image, Some("c.png".to_string()));
}

#[test]
fn test_image_set_quoted_and_prefixed_forms() {
    let quoted = computed_style(
        "",
        "background-image: image-set(\"hero.png\" 1x, \"hero@2x.png\" 2x);",
    );
    assert_eq!(quoted.background_image, Some("hero.png".to_string()));
    let prefixed = computed_style(
        "",
        "background-image: -webkit-image-set(url(w.png) 1x, url(w2.png) 2x);",
    );
    assert_eq!(prefixed.background_image, Some("w.png".to_string()));
}

#[test]
fn test_image_set_without_url_paints_nothing() {
    let style = computed_style("", "background-image: image-set(42px 1x);");
    assert_eq!(style.background_image, None);
    assert!(style.background_layers[0].image.is_none());
}

#[test]
fn test_image_set_in_multi_layer_list() {
    let style = computed_style(
        "",
        "background-image: image-set(url(top.png) 1x 2x, url(alt.png) 2x), \
         linear-gradient(red, blue);",
    );
    assert_eq!(style.background_image, Some("top.png".to_string()));
    assert_eq!(style.background_layers.len(), 2);
    assert!(style.background_layers[1].gradient.is_some());
}

#[test]
fn test_image_set_ties_keep_source_order() {
    let style = computed_style(
        "",
        "background-image: image-set(url(first.png) 1x, url(second.png) 1x);",
    );
    assert_eq!(style.background_image, Some("first.png".to_string()));
}

#[test]
fn test_backdrop_filter_is_ignored_and_filter_none_drops_cleanly() {
    // `backdrop-filter` is not modelled (documented): it must not populate `filter`.
    let style = computed_style("", "backdrop-filter: blur(4px); filter: none;");
    assert!(style.filter.is_empty());
    // A trailing comma/`none` combination is invalid and falls back to none.
    let invalid = computed_style("", "filter: grayscale(1),;");
    assert!(invalid.filter.is_empty());
}

#[test]
fn test_multiple_drop_shadows() {
    let style = computed_style(
        "",
        "filter: drop-shadow(1px 2px red) drop-shadow(3px 4px 5px #00ff00);",
    );
    assert_eq!(style.filter.len(), 2);
    assert!(matches!(
        style.filter[0],
        FilterFunction::DropShadow { offset_x, offset_y, blur: 0.0, .. }
            if offset_x == 1.0 && offset_y == 2.0
    ));
    assert!(matches!(
        style.filter[1],
        FilterFunction::DropShadow { blur: 5.0, .. }
    ));
}
