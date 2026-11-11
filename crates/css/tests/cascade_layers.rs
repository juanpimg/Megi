//! CSS Cascade Layers MVP: declaration order, nesting, anonymous layers, media/container
//! interaction, `!important` inversion and `revert-layer`.

use std::collections::HashMap;

use css::{ComputedStyle, Length, Origin, StyleCollection};
use dom::{Attribute, Document, Namespace, NodeId};

fn page(css: &str) -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let target = doc.create_element(Namespace::Html, "div", vec![Attribute::new("class", "x")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, target);
    let styles = css::resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    (doc, styles, target)
}

fn style(css: &str) -> ComputedStyle {
    let (_, styles, target) = page(css);
    styles.get(&target).expect("style").clone()
}

fn collection(css: &str) -> StyleCollection {
    let mut collection = StyleCollection::new();
    collection.add_css(css, Origin::Author);
    collection
}

#[test]
fn later_layer_wins_for_equal_specificity() {
    let style = style(
        "@layer base, theme;
         @layer base { .x { width: 10px; } }
         @layer theme { .x { width: 20px; } }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn explicit_statement_order_is_respected() {
    let style = style(
        "@layer theme, base;
         @layer theme { .x { width: 20px; } }
         @layer base { .x { width: 10px; } }",
    );
    assert_eq!(style.width, Length::Px(10.0));
}

#[test]
fn unlayered_normal_rules_beat_layers() {
    let style = style(
        ".x { width: 30px; }
         @layer base { .x { width: 10px; } }",
    );
    assert_eq!(style.width, Length::Px(30.0));
}

#[test]
fn important_inverts_layer_order() {
    let style = style(
        "@layer base, theme;
         @layer base { .x { width: 10px !important; } }
         @layer theme { .x { width: 20px !important; } }",
    );
    assert_eq!(style.width, Length::Px(10.0));
}

#[test]
fn important_layered_beats_normal_unlayered() {
    let style = style(
        ".x { width: 30px; }
         @layer base { .x { width: 10px !important; } }",
    );
    assert_eq!(style.width, Length::Px(10.0));
}

#[test]
fn first_appearance_order_of_anonymous_layers() {
    let style = style(
        "@layer { .x { width: 10px; } }
         @layer { .x { width: 20px; } }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn anonymous_and_named_layers_share_first_appearance_order() {
    let style = style(
        "@layer named { .x { width: 10px; } }
         @layer { .x { width: 20px; } }
         @layer named { .x { width: 30px; } }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn compound_layer_names_are_ordered() {
    let style = style(
        "@layer framework.base { .x { width: 10px; } }
         @layer framework.theme { .x { width: 20px; } }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn nested_layer_rules_are_orderable() {
    let style = style(
        "@layer outer {
           @layer inner { .x { width: 10px; } }
           .x { width: 5px; }
         }
         @layer outer.inner { .x { width: 20px; } }",
    );
    // `outer.inner` is declared after the direct rule in `outer`, so it wins; the direct
    // rule is approximated as part of `outer` itself (documented MVP limitation).
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn media_rules_inside_layers_keep_layer_order() {
    let style = style(
        "@layer base { .x { width: 10px; } }
         @layer theme { @media screen { .x { width: 20px; } } }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn layer_collection_exposes_first_appearance_order() {
    let collection = collection("@layer a, b; @layer c { } @layer a { }");
    let names: Vec<Vec<String>> = collection.layers.clone();
    assert_eq!(
        names,
        vec![
            vec!["a".to_owned()],
            vec!["b".to_owned()],
            vec!["c".to_owned()]
        ]
    );
}

#[test]
fn revert_layer_rolls_back_to_previous_layer() {
    let style = style(
        "@layer base, theme;
         @layer base { .x { width: 10px; color: red; } }
         @layer theme { .x { width: revert-layer; color: blue; } }",
    );
    assert_eq!(style.width, Length::Px(10.0));
}

#[test]
fn revert_layer_without_previous_layer_is_initial_not_ua() {
    let style = style("@layer only { .x { width: revert-layer; } }");
    assert_eq!(style.width, Length::Auto);
}

#[test]
fn revert_layer_skips_same_layer_candidates() {
    let style = style("@layer base { .x { width: 10px; } .x { width: revert-layer; } }");
    assert_eq!(style.width, Length::Auto);
}

#[test]
fn revert_layer_in_important_rolls_forward_again() {
    let style = style(
        "@layer base, theme;
         @layer base { .x { width: 10px !important; } }
         @layer theme { .x { width: revert-layer !important; } }",
    );
    assert_eq!(style.width, Length::Px(10.0));
}

#[test]
fn inline_style_wins_over_layers() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let target = doc.create_element(
        Namespace::Html,
        "div",
        vec![
            Attribute::new("class", "x"),
            Attribute::new("style", "width: 30px"),
        ],
    );
    doc.append_child(doc.root(), html);
    doc.append_child(html, target);
    let styles = css::resolve_document_styles_with_viewport(
        &doc,
        Some("@layer base { .x { width: 10px; } }"),
        800.0,
        600.0,
    );
    assert_eq!(styles[&target].width, Length::Px(30.0));
}

#[test]
fn no_layer_document_is_unchanged() {
    let plain = style(".x { width: 10px; } .x { width: 20px; }");
    assert_eq!(plain.width, Length::Px(20.0));
    let collection = collection(".x { width: 10px; }");
    assert!(collection.layers.is_empty());
}

#[test]
fn media_wrapping_a_layer_keeps_the_layer_order() {
    let style = style(
        "@media screen {
           @layer base { .x { width: 10px; } }
           @layer theme { .x { width: 20px; } }
         }",
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn container_rules_inside_a_layer_keep_the_layer_order() {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let container =
        doc.create_element(Namespace::Html, "div", vec![Attribute::new("class", "box")]);
    let target = doc.create_element(Namespace::Html, "div", vec![Attribute::new("class", "x")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, container);
    doc.append_child(container, target);
    let styles = css::resolve_document_styles_with_viewport(
        &doc,
        Some(
            ".box { width: 300px; container-type: inline-size; }
             @layer base { @container (min-width: 250px) { .x { width: 10px; } } }
             @layer theme { .x { width: 20px; } }",
        ),
        800.0,
        600.0,
    );
    assert_eq!(styles[&target].width, Length::Px(20.0));
}
