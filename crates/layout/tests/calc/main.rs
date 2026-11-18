//! Mixed `calc()` (`<percentage> + <length>`) resolution in layout.

use std::collections::HashMap;

use megi_core::{Document, Namespace, NodeId};
use megi_css::{ComputedStyle, Display, Length, Position};
use megi_layout::{BoxType, LayoutBox, Rect, layout_document};

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn block(width: Length, height: Length) -> ComputedStyle {
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

fn add(
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

fn find_box(root: &LayoutBox, id: NodeId) -> &LayoutBox {
    find_box_opt(root, id).unwrap_or_else(|| panic!("box {id:?} not found"))
}

#[derive(Clone, Copy, Debug)]
struct BoxDimensions {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn dimensions(layout_box: &LayoutBox) -> BoxDimensions {
    BoxDimensions {
        x: layout_box.dimensions.content.x,
        y: layout_box.dimensions.content.y,
        width: layout_box.dimensions.content.width,
        height: layout_box.dimensions.content.height,
    }
}

// The helper above is intentionally simple: most tests build the tree inline below.
fn simple(style: ComputedStyle, viewport_width: f32) -> BoxDimensions {
    let (mut doc, mut styles, html) = setup();
    let id = add(&mut doc, html, &mut styles, style);
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, viewport_width, 600.0)).expect("layout");
    dimensions(find_box(&root, id))
}

mod containers;
mod contexts;
mod length;
