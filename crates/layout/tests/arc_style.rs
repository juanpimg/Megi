//! `LayoutBox::style` is shared through `Arc<ComputedStyle>`: element boxes own their
//! handle once and generated boxes (text fragments, anonymous blocks, line boxes) re-use
//! the parent's `Arc` instead of deep-cloning a whole `ComputedStyle`.

use std::collections::HashMap;

use css::{ComputedStyle, Display};
use dom::{Document, Namespace, NodeId};
use layout::{BoxType, Rect, layout_document};

fn element(doc: &mut Document, parent: NodeId, name: &str) -> NodeId {
    let id = doc.create_element(Namespace::Html, name, vec![]);
    doc.append_child(parent, id);
    id
}

fn text(doc: &mut Document, parent: NodeId, data: &str) -> NodeId {
    let id = doc.create_text_node(data);
    doc.append_child(parent, id);
    id
}

fn block(id: NodeId) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    let _ = id;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = element(&mut doc, root, "html");
    let body = element(&mut doc, html, "body");
    let mut styles = HashMap::new();
    styles.insert(html, block(html));
    styles.insert(body, block(body));
    (doc, styles)
}

#[test]
fn test_sibling_text_boxes_share_the_parent_anonymous_style_arc() {
    let (mut doc, mut styles) = setup();
    let body = doc.children(doc.root()).next().unwrap();
    let body = doc.children(body).next().unwrap();
    let p = element(&mut doc, body, "p");
    styles.insert(p, block(p));
    text(&mut doc, p, "hola");
    // A styled inline element between the two text runs keeps them as separate
    // fragments (adjacent same-style text would be coalesced into one fragment).
    let span = element(&mut doc, p, "span");
    styles.insert(span, block(span));
    text(&mut doc, span, "x");
    text(&mut doc, p, "mundo");

    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).unwrap();
    let p_box = &tree.children[0].children[0];
    assert!(matches!(p_box.box_type, BoxType::BlockNode(_)));
    let mut text_boxes = Vec::new();
    collect_text_boxes(p_box, &mut text_boxes);
    assert_eq!(text_boxes.len(), 3, "hola + x + mundo");
    let first = text_boxes[0];
    let last = text_boxes[2];
    // The anonymous/inherited style is derived once per parent and shared.
    assert!(
        std::sync::Arc::ptr_eq(
            first.style.as_ref().expect("text box has a style"),
            last.style.as_ref().expect("text box has a style"),
        ),
        "sibling text boxes must share the parent's anonymous style Arc"
    );
    // Element boxes keep their own resolved style handle.
    assert!(!std::sync::Arc::ptr_eq(
        p_box.style.as_ref().unwrap(),
        first.style.as_ref().unwrap(),
    ));
    assert!(p_box.style().is_some());
    // The span has a different (own) style, so its text run must not share the Arc.
    let x_box = text_boxes[1];
    assert!(!std::sync::Arc::ptr_eq(
        first.style.as_ref().unwrap(),
        x_box.style.as_ref().unwrap(),
    ));
}

/// Collects every text box under `node` (tree order).
fn collect_text_boxes<'a>(node: &'a layout::LayoutBox, out: &mut Vec<&'a layout::LayoutBox>) {
    if matches!(node.box_type, BoxType::TextNode(_)) {
        out.push(node);
    }
    for child in &node.children {
        collect_text_boxes(child, out);
    }
}

#[test]
fn test_anonymous_block_shares_the_inherited_style_arc() {
    let (mut doc, mut styles) = setup();
    let body = doc.children(doc.root()).next().unwrap();
    let body = doc.children(body).next().unwrap();
    let mut body_style = block(body);
    body_style.display = Display::Block;
    styles.insert(body, body_style);
    // Mixed content forces an anonymous block around the inline run.
    text(&mut doc, body, "leading text");
    let div = element(&mut doc, body, "div");
    styles.insert(div, block(div));
    text(&mut doc, div, "block text");

    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).unwrap();
    let body_box = &tree.children[0];
    let anon = &body_box.children[0];
    assert!(matches!(anon.box_type, BoxType::AnonymousBlock));
    let mut text_boxes = Vec::new();
    collect_text_boxes(anon, &mut text_boxes);
    let text_box = *text_boxes
        .first()
        .expect("anonymous block wraps a text fragment");
    assert!(
        std::sync::Arc::ptr_eq(
            anon.style.as_ref().expect("anonymous block has a style"),
            text_box.style.as_ref().expect("text box has a style"),
        ),
        "the anonymous block and its text box share the derived inherited style"
    );
}
