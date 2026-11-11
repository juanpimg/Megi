//! Container Queries MVP: computed properties, parser/evaluator and cascade application.

use std::collections::HashMap;

use css::{ComputedStyle, ContainerType, Length};
use dom::{Attribute, Document, Namespace, NodeId};

fn element(doc: &mut Document, tag: &str, attributes: Vec<Attribute>) -> NodeId {
    doc.create_element(Namespace::Html, tag, attributes)
}

struct Page {
    styles: HashMap<NodeId, ComputedStyle>,
    container: NodeId,
    child: NodeId,
}

fn page(css: &str, viewport_width: f32) -> Page {
    let mut doc = Document::new();
    let html = element(&mut doc, "html", vec![]);
    let body = element(&mut doc, "body", vec![]);
    let container = element(&mut doc, "div", vec![Attribute::new("class", "box")]);
    let child = element(&mut doc, "div", vec![Attribute::new("class", "child")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, container);
    doc.append_child(container, child);
    let styles = css::resolve_document_styles_with_viewport(&doc, Some(css), viewport_width, 600.0);
    Page {
        styles,
        container,
        child,
    }
}

fn style(page: &Page, node: NodeId) -> &ComputedStyle {
    page.styles.get(&node).expect("style")
}

#[test]
fn parses_container_type_and_names() {
    let page = page(
        ".box { container-type: inline-size; container-name: card sidebar; }",
        800.0,
    );
    assert_eq!(
        style(&page, page.container).container_type,
        ContainerType::InlineSize
    );
    assert_eq!(
        style(&page, page.container).container_names,
        vec!["card".to_owned(), "sidebar".to_owned()]
    );
    assert_eq!(
        style(&page, page.child).container_type,
        ContainerType::Normal
    );
    assert!(style(&page, page.child).container_names.is_empty());
}

#[test]
fn min_width_query_applies_against_inline_size_container() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container (min-width: 250px) { .child { width: 111px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Px(111.0));
}

#[test]
fn max_width_and_and_conditions() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container (min-width: 100px) and (max-width: 400px) { .child { width: 222px; } }
         @container (min-width: 350px) and (max-width: 400px) { .child { height: 9px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Px(222.0));
    assert_eq!(style(&page, page.child).height, Length::Auto);
}

#[test]
fn without_a_container_the_rule_does_not_apply() {
    let page = page(
        "@container (min-width: 1px) { .child { width: 321px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Auto);
}

#[test]
fn named_queries_require_a_matching_container_name() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; container-name: card; }
         @container card (min-width: 100px) { .child { width: 123px; } }
         @container other (min-width: 100px) { .child { height: 45px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Px(123.0));
    assert_eq!(style(&page, page.child).height, Length::Auto);
}

#[test]
fn size_container_supports_height_queries() {
    let page = page(
        ".box { width: 300px; height: 200px; container-type: size; }
         @container (min-height: 150px) { .child { width: 77px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Px(77.0));
}

#[test]
fn inline_size_container_rejects_height_queries() {
    let page = page(
        ".box { width: 300px; height: 200px; container-type: inline-size; }
         @container (min-height: 1px) { .child { width: 88px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Auto);
}

#[test]
fn nearest_nested_container_wins() {
    let mut doc = Document::new();
    let html = element(&mut doc, "html", vec![]);
    let body = element(&mut doc, "body", vec![]);
    let outer = element(&mut doc, "div", vec![Attribute::new("class", "outer")]);
    let inner = element(&mut doc, "div", vec![Attribute::new("class", "inner")]);
    let child = element(&mut doc, "div", vec![Attribute::new("class", "child")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, outer);
    doc.append_child(outer, inner);
    doc.append_child(inner, child);
    let styles = css::resolve_document_styles_with_viewport(
        &doc,
        Some(
            ".outer { width: 500px; container-type: inline-size; container-name: outer; }
             .inner { width: 250px; container-type: inline-size; container-name: inner; }
             @container inner (max-width: 300px) { .child { width: 64px; } }
             @container outer (min-width: 400px) { .child { height: 12px; } }",
        ),
        800.0,
        600.0,
    );
    assert_eq!(styles[&child].width, Length::Px(64.0));
    assert_eq!(styles[&child].height, Length::Px(12.0));
}

#[test]
fn invalid_or_unsupported_queries_are_ignored() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container style(--x: 1) { .child { width: 999px; } }
         @container (min-width: nope) { .child { height: 999px; } }
         @container (min-width: 100px) and (max-height: 1px) { .child { margin-left: 9px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Auto);
    assert_eq!(style(&page, page.child).height, Length::Auto);
    assert_eq!(style(&page, page.child).margin.left, Length::Px(0.0));
}

#[test]
fn container_query_lengths_support_percent_and_em() {
    let page = page(
        ".box { width: 400px; font-size: 20px; container-type: inline-size; }
         @container (min-width: 50%) { .child { width: 10px; } }
         @container (min-width: 10em) { .child { height: 20px; } }",
        800.0,
    );
    assert_eq!(style(&page, page.child).width, Length::Px(10.0));
    assert_eq!(style(&page, page.child).height, Length::Px(20.0));
}

#[test]
fn container_rules_do_not_leak_without_queries() {
    let page = page(".box { width: 300px; } .child { width: 42px; }", 800.0);
    assert_eq!(style(&page, page.child).width, Length::Px(42.0));
}
