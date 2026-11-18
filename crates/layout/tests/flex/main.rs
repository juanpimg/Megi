//! Flexbox MVP layout tests (`display: flex` / `inline-flex`).
//!
//! The DOM is built by hand so the tests are independent from the HTML parser and
//! exercise the layout algorithm directly. All supported properties from
//! `crates/layout/src/flex.rs` are covered: directions, justify-content, align
//! items/self (including stretch and baseline), flex-grow/shrink/basis, gap,
//! wrapping, inline-flex and item BFC content.

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{
    AlignItems, AlignSelf, ComputedStyle, Display, FlexDirection, FlexWrap, JustifyContent, Length,
};
use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn flex_container_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.display = Display::Flex;
    style.width = width;
    style.height = height;
    style
}

fn item_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = block_style();
    style.width = width;
    style.height = height;
    style
}

fn setup() -> (Document, HashMap<NodeId, ComputedStyle>, NodeId) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let mut styles = HashMap::new();
    styles.insert(html, block_style());
    (doc, styles, html)
}

fn add_element(
    doc: &mut Document,
    parent: NodeId,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    style: ComputedStyle,
) -> NodeId {
    let id = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(parent, id);
    styles.insert(id, style);
    id
}

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return root;
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return found;
        }
    }
    panic!("layout box for node {id:?} not found");
}

fn find_box_opt(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    if matches!(root.box_type, BoxType::BlockNode(node) | BoxType::InlineBlock(node) if node == id)
    {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_box_opt(child, id) {
            return Some(found);
        }
    }
    None
}

struct Case {
    root: LayoutBox,
    ids: Vec<NodeId>,
}

fn layout_case(container: ComputedStyle, items: Vec<ComputedStyle>) -> Case {
    let (mut doc, mut styles, html) = setup();
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let mut ids = Vec::new();
    for item in items {
        ids.push(add_element(&mut doc, container_id, &mut styles, item));
    }
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout result");
    Case { root, ids }
}

impl Case {
    fn item(&self, index: usize) -> &LayoutBox {
        find_box(&self.root, self.ids[index])
    }
}

mod alignment;
mod placement;
