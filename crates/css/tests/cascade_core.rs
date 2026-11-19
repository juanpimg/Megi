//! #1360: direct tests for the cascade core that were previously exercised only indirectly
//! through layout/render: numeric specificity, Selectors-4 pseudo-classes, and the
//! `animation-*`/`transition-*` longhands.

use std::collections::HashMap;

use megi_core::{Attribute, Document, Namespace, NodeId};
use megi_css::selector::{Specificity, parse_selectors};
use megi_css::{ComputedStyle, Length, Origin, StyleCollection};

fn append(doc: &mut Document, parent: NodeId, name: &str, attrs: &[(&str, &str)]) -> NodeId {
    let attributes = attrs
        .iter()
        .map(|(key, value)| Attribute::new(*key, *value))
        .collect();
    let node = doc.create_element(Namespace::Html, name, attributes);
    doc.append_child(parent, node);
    node
}

fn page() -> (Document, NodeId, HashMap<NodeId, ComputedStyle>) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    let target = append(&mut doc, body, "div", &[("class", "x"), ("id", "t")]);
    let styles = megi_css::resolve_document_styles(&doc, Some(""));
    (doc, target, styles)
}

fn specificity(selector: &str) -> Specificity {
    parse_selectors(selector)
        .selectors
        .first()
        .expect("selector")
        .specificity()
}

#[test]
fn numeric_specificity_orders_id_class_type() {
    assert!(specificity("#t") > specificity(".x"));
    assert!(specificity(".x") > specificity("div"));
    assert!(specificity("div.x") > specificity(".x"));
    assert_eq!(specificity("#t.x"), Specificity::new(1, 1, 0));
}

#[test]
fn is_not_has_use_most_specific_argument_where_is_zero() {
    assert_eq!(specificity(":is(#t, .x)"), Specificity::new(1, 0, 0));
    assert_eq!(specificity(":where(#t, .x)"), Specificity::new(0, 0, 0));
    assert_eq!(specificity(":not(.x)"), Specificity::new(0, 1, 0));
    assert_eq!(specificity(":has(> div)"), Specificity::new(0, 0, 1));
}

#[test]
fn cascade_prefers_higher_specificity_regardless_of_order() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    let target = append(&mut doc, body, "div", &[("class", "x"), ("id", "t")]);
    let styles = megi_css::resolve_document_styles_with_viewport(
        &doc,
        Some("div { width: 5px; } #t { width: 10px; } .x { width: 7px; }"),
        800.0,
        600.0,
    );
    assert_eq!(styles.get(&target).expect("style").width, Length::Px(10.0));
}

#[test]
fn pseudo_class_selectors_match_the_target() {
    let (doc, target, _) = page();
    for selector in [":is(.x)", ":not(.y)", "div.x", "#t"] {
        assert!(
            parse_selectors(selector).matches(&doc, target),
            "{selector} must match"
        );
    }
}

#[test]
fn animation_and_transition_longhands_are_computed() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    let target = append(&mut doc, body, "div", &[("class", "x")]);
    let styles = megi_css::resolve_document_styles_with_viewport(
        &doc,
        Some(
            ".x { animation-name: fade; animation-duration: 250ms; \
                 transition-property: opacity; transition-duration: 100ms; }",
        ),
        800.0,
        600.0,
    );
    let style = styles.get(&target).expect("style");
    assert_eq!(style.animations.first().expect("animation").name, "fade");
    assert_eq!(
        style.animations.first().expect("animation").duration_ms,
        250.0
    );
    assert!(!style.transitions.is_empty());
}

#[test]
fn style_collection_records_rules_for_each_group() {
    let mut collection = StyleCollection::new();
    collection.add_css("div { width: 1px; } span { width: 2px; }", Origin::Author);
    assert_eq!(collection.rules.len(), 2);
    assert_eq!(collection.rules[0].declarations.len(), 1);
    assert_eq!(collection.rules[0].declarations[0].name, "width");
    assert_eq!(collection.rules[0].declarations[0].raw_value, "1px");
    assert_eq!(collection.rules[1].declarations.len(), 1);
    assert_eq!(collection.rules[1].declarations[0].name, "width");
    assert_eq!(collection.rules[1].declarations[0].raw_value, "2px");

    // Parsing a stylesheet is independent from recording rules into a collection.
    let stylesheet = megi_css::parse_stylesheet("div { color: red; }");
    assert_eq!(stylesheet.rules.len(), 1);
}

#[test]
fn lang_pseudo_class_matches_inherited_language_and_prefix_ranges() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[("lang", "en-GB")]);
    let body = append(&mut doc, html, "body", &[]);
    let target = append(&mut doc, body, "p", &[]);
    // `:lang(en)` matches the inherited `en-GB`; `:lang(en-GB)` matches exactly;
    // `:lang(fr)` does not match.
    for (selector, expected) in [
        (":lang(en)", true),
        (":lang(en-gb)", true),
        (":lang(fr)", false),
    ] {
        assert_eq!(
            parse_selectors(selector).matches(&doc, target),
            expected,
            "{selector}"
        );
    }
    let styles = megi_css::resolve_document_styles_with_viewport(
        &doc,
        Some(":lang(fr) { width: 1px; } :lang(en) { width: 9px; }"),
        800.0,
        600.0,
    );
    assert_eq!(styles.get(&target).expect("style").width, Length::Px(9.0));
}
