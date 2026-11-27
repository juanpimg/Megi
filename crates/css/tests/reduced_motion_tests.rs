//! Media Queries 5 §5.4: prefers-reduced-motion accepts reduce | no-preference.

use megi_core::{Document, Namespace};
use megi_css::{ComputedStyle, Display, MediaContext, resolve_document_styles_with_media};

fn style_for(css: &str, reduced: bool) -> ComputedStyle {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html".to_owned(), vec![]);
    doc.append_child(doc.root(), html);
    let target = doc.create_element(
        Namespace::Html,
        "div".to_owned(),
        vec![megi_core::Attribute::new("id".to_owned(), "x".to_owned())],
    );
    doc.append_child(html, target);
    let mut ctx = MediaContext::with_viewport(800.0, 600.0);
    ctx.reduced_motion = reduced;
    let styles = resolve_document_styles_with_media(&doc, Some(css), ctx);
    styles.get(&target).cloned().unwrap_or_default()
}

#[test]
fn no_preference_matches_when_user_has_not_requested_reduction() {
    let s = style_for(
        "@media (prefers-reduced-motion: no-preference) { #x { display: none } }",
        false,
    );
    assert_eq!(
        s.display,
        Display::None,
        "no-preference must match the default profile"
    );
}

#[test]
fn no_preference_does_not_match_when_user_requests_reduction() {
    let s = style_for(
        "@media (prefers-reduced-motion: no-preference) { #x { display: none } }",
        true,
    );
    assert_ne!(s.display, Display::None);
}

#[test]
fn reduce_still_matches_only_when_requested() {
    let css = "@media (prefers-reduced-motion: reduce) { #x { display: none } }";
    assert_eq!(style_for(css, true).display, Display::None);
    assert_ne!(style_for(css, false).display, Display::None);
}

#[test]
fn invalid_value_is_rejected() {
    let s = style_for(
        "@media (prefers-reduced-motion: bogus) { #x { display: none } }",
        false,
    );
    assert_ne!(s.display, Display::None);
}
