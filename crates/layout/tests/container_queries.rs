//! Container Queries MVP geometry: the resolved `@container` styles must drive exact
//! block geometry once the layout pass runs against the predicted container size.

use std::collections::HashMap;

use css::ComputedStyle;
use dom::{Attribute, Document, Namespace, NodeId};
use layout::{LayoutBox, Rect, layout_document};

struct Page {
    doc: Document,
    styles: HashMap<NodeId, ComputedStyle>,
    child: NodeId,
}

fn element(doc: &mut Document, tag: &str, attributes: Vec<Attribute>) -> NodeId {
    doc.create_element(Namespace::Html, tag, attributes)
}

fn page(css: &str) -> Page {
    let mut doc = Document::new();
    let html = element(&mut doc, "html", vec![]);
    let body = element(&mut doc, "body", vec![]);
    let container = element(&mut doc, "div", vec![Attribute::new("class", "box")]);
    let child = element(&mut doc, "div", vec![Attribute::new("class", "child")]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, container);
    doc.append_child(container, child);
    let styles = css::resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
    Page { doc, styles, child }
}

fn find(node: &LayoutBox, target: NodeId) -> Option<&LayoutBox> {
    if node.node_id() == Some(target) {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, target))
}

#[test]
fn inline_size_container_changes_child_width() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container (min-width: 250px) { .child { width: 100px; height: 40px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 100.0);
    assert_eq!(child.dimensions.content.height, 40.0);
}

#[test]
fn query_not_matching_keeps_auto_width() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container (min-width: 350px) { .child { width: 222px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 300.0);
}

#[test]
fn max_width_and_and_conditions_are_exact() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; }
         @container (max-width: 320px) and (min-width: 280px) { .child { width: 64px; height: 17px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 64.0);
    assert_eq!(child.dimensions.content.height, 17.0);
}

#[test]
fn no_ancestor_container_does_not_apply() {
    let page = page(
        ".box { width: 300px; }
         @container (min-width: 1px) { .child { width: 100px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 300.0);
}

#[test]
fn named_container_matches_and_sibling_name_does_not() {
    let page = page(
        ".box { width: 300px; container-type: inline-size; container-name: card; }
         @container card (min-width: 100px) { .child { width: 123px; } }
         @container other (min-width: 1px) { .child { height: 99px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 123.0);
    assert_eq!(child.dimensions.content.height, 0.0);
}

#[test]
fn size_container_height_query_reaches_child_geometry() {
    let page = page(
        ".box { width: 300px; height: 220px; container-type: size; }
         @container (min-height: 200px) { .child { width: 70px; height: 30px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 70.0);
    assert_eq!(child.dimensions.content.height, 30.0);
}

#[test]
fn inline_size_container_ignores_height_queries() {
    let page = page(
        ".box { width: 300px; height: 220px; container-type: inline-size; }
         @container (min-height: 1px) { .child { width: 88px; } }",
    );
    let tree = layout_document(&page.doc, &page.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let child = find(&tree, page.child).expect("child box");
    assert_eq!(child.dimensions.content.width, 300.0);
}

#[test]
fn nested_containers_use_the_nearest_eligible_one() {
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
             @container inner (max-width: 300px) { .child { width: 64px; } }",
        ),
        800.0,
        600.0,
    );
    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let child_box = find(&tree, child).expect("child box");
    assert_eq!(child_box.dimensions.content.width, 64.0);
}

#[test]
fn container_queries_do_not_change_geometry_when_they_do_not_match() {
    let plain = page(".box { width: 300px; } .child { width: 80px; height: 20px; }");
    let queried = page(
        ".box { width: 300px; container-type: inline-size; }
         .child { width: 80px; height: 20px; }
         @container (min-width: 1000px) { .child { width: 1px; } }",
    );
    let a = layout_document(&plain.doc, &plain.styles, Rect::new(0.0, 0.0, 800.0, 600.0))
        .expect("layout");
    let b = layout_document(
        &queried.doc,
        &queried.styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
    )
    .expect("layout");
    let ca = find(&a, plain.child).expect("child a");
    let cb = find(&b, queried.child).expect("child b");
    assert_eq!(ca.dimensions.content.width, cb.dimensions.content.width);
    assert_eq!(ca.dimensions.content.height, cb.dimensions.content.height);
    assert_eq!(ca.dimensions.content.width, 80.0);
}
