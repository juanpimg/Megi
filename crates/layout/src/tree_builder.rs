//! # Layout Box Tree Construction
//!
//! Transforms a [`megi_core::Document`] and its resolved [`megi_css::ComputedStyle`] map into
//! a conformant CSS Box Tree per CSS 2.2 §9 and CSS Display Module Level 3.

use std::collections::HashMap;
use std::sync::Arc;

use megi_core::{Document, Namespace, NodeId, NodeKind};
use megi_css::{
    ComputedStyle, ContentItem, Display, Float, Overflow, Position, PseudoElement, PseudoStyles,
};

use crate::box_tree::{BoxType, LayoutBox, anonymous_style_from, is_collapsible_whitespace_box};

mod box_for_node;
mod cache;
mod fixups;

use box_for_node::build_box_for_node;
use cache::{CounterScope, StyleCache};

/// Builds the layout box tree for the document root element.
pub fn build_layout_tree(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Option<LayoutBox> {
    build_layout_tree_with_replaced_sizes(doc, styles, None)
}

/// Builds the layout box tree with explicit intrinsic sizes for replaced elements
/// (decoded image dimensions, form-control sizes, …), keyed by DOM node.
///
/// Nodes missing from the map fall back to HTML/CSS 2.2 per-kind defaults
/// (e.g. `<img>` 300×150, `<input>` 20×20).
pub fn build_layout_tree_with_replaced_sizes(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
) -> Option<LayoutBox> {
    build_layout_tree_internal(doc, styles, None, replaced_sizes).map(|(root, _)| root)
}

/// Builds the layout box tree with pseudo-element (`::before` / `::after`) styles and
/// explicit intrinsic sizes for replaced elements.
pub fn build_layout_tree_with_pseudos(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudo_styles: &PseudoStyles,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
) -> Option<LayoutBox> {
    build_layout_tree_internal(doc, styles, Some(pseudo_styles), replaced_sizes)
        .map(|(root, _)| root)
}

pub(crate) fn build_layout_tree_internal(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudo_styles: Option<&PseudoStyles>,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
) -> Option<(LayoutBox, bool)> {
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
    let mut has_overflow_clip = false;
    let mut style_cache = StyleCache::default();
    let mut counters = CounterScope::default();
    let active_pseudos = pseudo_styles.filter(|map| !map.is_empty());
    let root = build_box_for_node(
        doc,
        root_node_id,
        styles,
        active_pseudos,
        None,
        replaced_sizes,
        &mut has_overflow_clip,
        &mut style_cache,
        &mut counters,
        0,
    )?;
    Some((root, has_overflow_clip))
}

/// Maximum DOM nesting depth traversed when constructing the layout box tree (#417).
const MAX_TREE_BUILD_DEPTH: usize = 256;
