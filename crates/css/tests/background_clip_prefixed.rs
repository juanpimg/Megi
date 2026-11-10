//! `background-clip: text` and the `-webkit-background-clip` alias (R106).
//!
//! The prefixed longhand is accepted as an alias of the unprefixed property
//! (browsers expose it for compatibility); the computed clip is used by the
//! painter to mask backgrounds through the element's glyphs.

use css::{BackgroundClip, resolve_document_styles};
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
fn test_unprefixed_background_clip_text_is_parsed() {
    let style = computed_style(
        "div { background-clip: text; background-image: url(x.png); }",
        "",
    );
    assert_eq!(style.background_layers.len(), 1);
    assert_eq!(style.background_layers[0].clip, BackgroundClip::Text);
}

#[test]
fn test_prefixed_background_clip_text_is_an_alias() {
    let style = computed_style(
        "div { -webkit-background-clip: text; background-image: url(x.png); }",
        "",
    );
    assert_eq!(style.background_layers[0].clip, BackgroundClip::Text);
}

#[test]
fn test_unprefixed_longhand_wins_over_prefixed() {
    let style = computed_style(
        "div { -webkit-background-clip: text; background-clip: padding-box; \
         background-image: url(x.png); }",
        "",
    );
    assert_eq!(style.background_layers[0].clip, BackgroundClip::PaddingBox);
}

#[test]
fn test_prefixed_in_inline_style() {
    let style = computed_style(
        "div { background-color: red; }",
        "-webkit-background-clip: text;",
    );
    assert_eq!(style.background_layers[0].clip, BackgroundClip::Text);
}

#[test]
fn test_prefixed_box_clip_variants() {
    let style = computed_style(
        "div { -webkit-background-clip: content-box; background-image: url(a.png); }",
        "",
    );
    assert_eq!(style.background_layers[0].clip, BackgroundClip::ContentBox);
}
