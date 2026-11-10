//! # Layout Box Tree Construction
//!
//! Transforms a [`dom::Document`] and its resolved [`css::ComputedStyle`] map into
//! a conformant CSS Box Tree per CSS 2.2 §9 and CSS Display Module Level 3.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Float, Position};
use dom::{Document, Namespace, NodeId, NodeKind};

use crate::box_tree::{BoxType, LayoutBox, anonymous_style_from, is_collapsible_whitespace_box};

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
    build_box_for_node(doc, root_node_id, styles, None)
}

/// Returns `true` if the node is an HTML `<br>` element.
fn is_html_br(node: &dom::Node) -> bool {
    node.as_element().is_some_and(|el| {
        el.namespace == Namespace::Html && el.local_name.eq_ignore_ascii_case("br")
    })
}

/// Recursively builds a layout box for a DOM node.
fn build_box_for_node(
    doc: &Document,
    node_id: NodeId,
    styles: &HashMap<NodeId, ComputedStyle>,
    parent_style: Option<&ComputedStyle>,
) -> Option<LayoutBox> {
    let node = doc.node(node_id);
    match &node.kind {
        NodeKind::Element(_) => {
            let style = styles.get(&node_id);
            // display: none elements generate no boxes and suppress descendant boxes
            if style.is_some_and(|style| style.display == Display::None) {
                return None;
            }

            // `<br>` is a forced line break: a formatting artifact with no box decoration.
            // It is handled before requiring a resolved style so a missing style entry
            // cannot silently drop an explicit line break.
            if is_html_br(node) {
                return Some(LayoutBox::new(BoxType::LineBreak, None));
            }

            let style = style?;

            // CSS 2.2 §9.7: `float` blockifies the box; §9.6/§9.3: absolutely
            // positioned boxes are out of flow and also blockified.
            let blockified = style.float != Float::None
                || matches!(style.position, Position::Absolute | Position::Fixed);
            let box_type = match style.display {
                Display::Block
                | Display::Table
                | Display::TableRow
                | Display::TableCell
                | Display::Flex
                | Display::Grid
                | Display::ListItem => BoxType::BlockNode(node_id),
                _ if blockified => BoxType::BlockNode(node_id),
                _ => BoxType::InlineNode(node_id),
            };

            let mut raw_children = Vec::new();
            let mut curr = node.first_child;
            while let Some(child_id) = curr {
                if let Some(child_box) = build_box_for_node(doc, child_id, styles, Some(style)) {
                    raw_children.push(child_box);
                }
                curr = doc.node(child_id).next_sibling;
            }

            let children = match box_type {
                BoxType::BlockNode(_) => normalize_block_children(raw_children, Some(style)),
                // CSS 2.2 §9.2.1.1: block-level children inside an inline box implicitly
                // break the inline around them. Full normalization is out of scope for
                // this milestone; the IFC flattens the inline subtree as-is.
                _ => raw_children,
            };

            let mut layout_box = LayoutBox::new(box_type, Some(style.clone()));
            // Data contract consumed by `paint` (stacking/clip metadata).
            layout_box.position = style.position;
            layout_box.z_index = if style.position == Position::Static {
                0
            } else {
                style.z_index.unwrap_or(0)
            };
            layout_box.opacity = style.opacity;
            layout_box.is_float = style.float != Float::None;
            layout_box.float_side = style.float;
            layout_box.children = children;
            Some(layout_box)
        }
        NodeKind::Text(txt) => {
            // An empty text node generates no inline content.
            if txt.is_empty() {
                return None;
            }
            // Text boxes are formatting artifacts: they must only carry inherited
            // properties, never the parent's margins/borders/background (F-02).
            let parent = parent_style.map(anonymous_style_from);
            Some(LayoutBox::new(BoxType::TextNode(txt.clone()), parent))
        }
        _ => None,
    }
}

/// Normalizes children inside a block container box per CSS 2.2 §9.2.1.1.
///
/// If a block container has both inline-level and block-level children, contiguous runs
/// of inline-level children are wrapped into an [`BoxType::AnonymousBlock`] box. Runs
/// consisting solely of collapsible whitespace are discarded: they are insignificant
/// between block-level siblings and would otherwise create phantom vertical gaps in
/// pretty-printed HTML (audit finding F-02).
fn normalize_block_children(
    children: Vec<LayoutBox>,
    parent_style: Option<&ComputedStyle>,
) -> Vec<LayoutBox> {
    let has_block = children.iter().any(|c| c.is_block());
    let has_inline = children.iter().any(|c| c.is_inline());

    // If pure blocks or pure inlines, no anonymous wrapping required
    if !has_block || !has_inline {
        return children;
    }

    let anonymous_style = parent_style.map(anonymous_style_from);
    let mut normalized = Vec::new();
    let mut current_inline_run: Vec<LayoutBox> = Vec::new();

    for child in children {
        if child.is_block() {
            flush_inline_run(&mut current_inline_run, &mut normalized, &anonymous_style);
            normalized.push(child);
        } else {
            current_inline_run.push(child);
        }
    }

    flush_inline_run(&mut current_inline_run, &mut normalized, &anonymous_style);
    normalized
}

/// Wraps a contiguous run of inline-level children in an anonymous block, unless the
/// run is made of collapsible whitespace only.
fn flush_inline_run(
    run: &mut Vec<LayoutBox>,
    normalized: &mut Vec<LayoutBox>,
    anonymous_style: &Option<ComputedStyle>,
) {
    if run.is_empty() {
        return;
    }
    let significant = run.iter().any(|c| !is_collapsible_whitespace_box(c));
    if significant {
        let mut anon = LayoutBox::new(BoxType::AnonymousBlock, anonymous_style.clone());
        anon.children = std::mem::take(run);
        normalized.push(anon);
    } else {
        run.clear();
    }
}
