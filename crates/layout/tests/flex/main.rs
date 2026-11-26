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
    AlignItems, AlignSelf, ComputedStyle, Display, FlexDirection, FlexWrap, Float, JustifyContent,
    Length,
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
mod min_auto;
mod placement;

#[test]
fn wiki_header_flex_ignores_float_on_items() {
    // CSS Flexbox 1 §4: `float` on an in-flow child of a flex container is
    // ignored — it stays a flex item. Wikipedia's titlebar language selector is
    // `float:right` inside a flex container; skipping it left the h1 alone at
    // the flex-end edge.
    let mut first = item_style(Length::Px(50.0), Length::Px(20.0));
    first.float = Float::Left;
    let second = item_style(Length::Px(50.0), Length::Px(20.0));
    let case = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(80.0)),
        vec![first, second],
    );
    assert!(
        !case.item(0).is_float,
        "a flex item must not be laid out as an out-of-flow float"
    );
    assert_eq!(
        case.item(1).dimensions.content.x,
        case.item(0).dimensions.content.x + 50.0,
        "both floated and non-floated children are flex items in tree order"
    );
}
