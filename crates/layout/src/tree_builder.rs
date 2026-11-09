//! # Layout Box Tree Construction
//!
//! Transforms a [`dom::Document`] and its resolved [`css::ComputedStyle`] map into
//! a conformant CSS Box Tree per CSS 2.2 §9 and CSS Display Module Level 3.

use std::collections::HashMap;

use css::{ComputedStyle, Display};
use dom::{Document, NodeId, NodeKind};

use crate::box_tree::{BoxType, LayoutBox};

/// Builds the layout box tree for the document root element.
pub fn build_layout_tree(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Option<LayoutBox> {
    // Find the <html> element under the document root
    let mut root_el_id = None;
    let mut curr = doc.node(doc.root()).first_child;
    while let Some(child_id) = curr {
        if doc.node(child_id).as_element().is_some() {
            root_el_id = Some(child_id);
            break;
        }
        curr = doc.node(child_id).next_sibling;
    }

    let root_node_id = root_el_id?;
    build_box_for_node(doc, root_node_id, styles)
}

/// Recursively builds a layout box for a DOM node.
fn build_box_for_node(
    doc: &Document,
    node_id: NodeId,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Option<LayoutBox> {
    let node = doc.node(node_id);
    match &node.kind {
        NodeKind::Element(_) => {
            let style = styles.get(&node_id)?;
            // display: none elements generate no boxes and suppress descendant boxes
            if style.display == Display::None {
                return None;
            }

            let box_type = match style.display {
                Display::Block
                | Display::Table
                | Display::TableRow
                | Display::TableCell
                | Display::Flex
                | Display::Grid
                | Display::ListItem => BoxType::BlockNode(node_id),
                _ => BoxType::InlineNode(node_id),
            };

            let mut raw_children = Vec::new();
            let mut curr = node.first_child;
            while let Some(child_id) = curr {
                if let Some(child_box) = build_box_for_node(doc, child_id, styles) {
                    raw_children.push(child_box);
                }
                curr = doc.node(child_id).next_sibling;
            }

            let children = if matches!(box_type, BoxType::BlockNode(_)) {
                normalize_block_children(raw_children)
            } else {
                raw_children
            };

            Some(LayoutBox {
                dimensions: Default::default(),
                box_type,
                style: Some(style.clone()),
                children,
            })
        }
        NodeKind::Text(txt) => {
            // Discard pure whitespace text nodes that are empty
            let trimmed = txt.trim_matches(|c: char| c == '\r' || c == '\n' || c == '\t');
            if trimmed.is_empty() {
                None
            } else {
                Some(LayoutBox::new(BoxType::TextNode(txt.clone()), None))
            }
        }
        _ => None,
    }
}

/// Normalizes children inside a block container box per CSS 2.2 §9.2.1.1.
///
/// If a block container has both inline-level and block-level children, contiguous runs
/// of inline-level children are wrapped into an [`BoxType::AnonymousBlock`] box.
fn normalize_block_children(children: Vec<LayoutBox>) -> Vec<LayoutBox> {
    let has_block = children.iter().any(|c| c.is_block());
    let has_inline = children.iter().any(|c| c.is_inline());

    // If pure blocks or pure inlines, no anonymous wrapping required
    if !has_block || !has_inline {
        return children;
    }

    let mut normalized = Vec::new();
    let mut current_inline_run: Vec<LayoutBox> = Vec::new();

    for child in children {
        if child.is_block() {
            if !current_inline_run.is_empty() {
                let mut anon = LayoutBox::new(BoxType::AnonymousBlock, None);
                anon.children = std::mem::take(&mut current_inline_run);
                normalized.push(anon);
            }
            normalized.push(child);
        } else {
            current_inline_run.push(child);
        }
    }

    if !current_inline_run.is_empty() {
        let mut anon = LayoutBox::new(BoxType::AnonymousBlock, None);
        anon.children = current_inline_run;
        normalized.push(anon);
    }

    normalized
}
