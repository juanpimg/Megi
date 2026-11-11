//! # Layout Box Tree Construction
//!
//! Transforms a [`dom::Document`] and its resolved [`css::ComputedStyle`] map into
//! a conformant CSS Box Tree per CSS 2.2 §9 and CSS Display Module Level 3.

use std::collections::HashMap;
use std::sync::Arc;

use css::{ComputedStyle, Display, Float, Overflow, Position};
use dom::{Document, Namespace, NodeId, NodeKind};

use crate::box_tree::{BoxType, LayoutBox, anonymous_style_from, is_collapsible_whitespace_box};

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
    build_layout_tree_internal(doc, styles, replaced_sizes).map(|(root, _)| root)
}

/// Builds the layout tree and reports whether any box establishes `overflow` clipping.
///
/// The information lets `layout_document` skip the clip-propagation post-pass for
/// documents without clipping boxes (the common case).
/// Per-layout-call cache that interns equal `ComputedStyle` values so that the
/// 28k boxes of a text-heavy document share the (few hundred) distinct styles
/// instead of deep-cloning one style per element. Equality is always checked on a
/// bucket hit, so the key (a cached value fingerprint) can never change semantics.
#[derive(Default)]
struct StyleCache {
    buckets: HashMap<u64, Vec<Arc<ComputedStyle>>>,
    anonymous: HashMap<usize, Arc<ComputedStyle>>,
}

impl StyleCache {
    fn intern(&mut self, style: &ComputedStyle) -> Arc<ComputedStyle> {
        // Styles produced by the cascade carry their value fingerprint (computed once in
        // `ComputedStyle::compute`), so interning does not re-hash the whole style per
        // element. Hand-built styles (tests, `initial()`: fingerprint `0`) fall back to
        // computing it on demand so equal hand-built styles still share.
        let cached = style.fingerprint();
        let key = if cached == 0 {
            style.compute_fingerprint()
        } else {
            cached
        };
        if let Some(bucket) = self.buckets.get(&key) {
            for candidate in bucket {
                if candidate.as_ref() == style {
                    return Arc::clone(candidate);
                }
            }
        }
        let arc = Arc::new(style.clone());
        self.buckets.entry(key).or_default().push(Arc::clone(&arc));
        arc
    }

    /// Anonymous (generated-box) style derived from a parent style. Parents that
    /// share an interned style share the derived anonymous style too.
    fn anonymous_for(&mut self, parent: &Arc<ComputedStyle>) -> Arc<ComputedStyle> {
        let key = Arc::as_ptr(parent) as usize;
        if let Some(style) = self.anonymous.get(&key) {
            return Arc::clone(style);
        }
        let style = Arc::new(anonymous_style_from(parent));
        self.anonymous.insert(key, Arc::clone(&style));
        style
    }
}

pub(crate) fn build_layout_tree_internal(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
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
    let root = build_box_for_node(
        doc,
        root_node_id,
        styles,
        None,
        replaced_sizes,
        &mut has_overflow_clip,
        &mut style_cache,
    )?;
    Some((root, has_overflow_clip))
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
    // Inherited style for generated boxes: the anonymous style derived once from the
    // parent element (shared `Arc`), used directly by text and anonymous boxes.
    inherited_style: Option<&Arc<ComputedStyle>>,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
    has_overflow_clip: &mut bool,
    style_cache: &mut StyleCache,
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
            let tag = node
                .as_element()
                .map(|el| el.local_name.as_str())
                .unwrap_or_default();
            // HTML parser lowercases tag names already; only allocate when a hand-built
            // DOM has uppercase ASCII in the element name.
            let tag = if tag.bytes().any(|byte| byte.is_ascii_uppercase()) {
                std::borrow::Cow::Owned(tag.to_ascii_lowercase())
            } else {
                std::borrow::Cow::Borrowed(tag)
            };

            // CSS 2.1 §17: table parts. `display` is authoritative; the HTML tag is a
            // fallback for documents whose UA stylesheet is incomplete (or for styles
            // built by hand in tests).
            let display_role = match style.display {
                Display::Table => Some(BoxType::Table(node_id)),
                Display::InlineTable => Some(BoxType::InlineTable(node_id)),
                Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => {
                    Some(BoxType::TableRowGroup(node_id))
                }
                Display::TableRow => Some(BoxType::TableRow(node_id)),
                Display::TableCell => Some(BoxType::TableCell(node_id)),
                Display::TableCaption => Some(BoxType::TableCaption(node_id)),
                Display::TableColumn | Display::TableColumnGroup => {
                    Some(BoxType::TableColumn(node_id))
                }
                Display::InlineBlock => Some(BoxType::InlineBlock(node_id)),
                _ => None,
            };
            let tag_role = match tag.as_ref() {
                "table" => Some(BoxType::Table(node_id)),
                "thead" | "tbody" | "tfoot" => Some(BoxType::TableRowGroup(node_id)),
                "tr" => Some(BoxType::TableRow(node_id)),
                "td" | "th" => Some(BoxType::TableCell(node_id)),
                "caption" => Some(BoxType::TableCaption(node_id)),
                "col" | "colgroup" => Some(BoxType::TableColumn(node_id)),
                _ => None,
            };
            let is_replaced = matches!(
                tag.as_ref(),
                "img"
                    | "input"
                    | "select"
                    | "textarea"
                    | "canvas"
                    | "video"
                    | "audio"
                    | "iframe"
                    | "embed"
                    | "object"
            );
            let box_type = if let Some(role) = display_role {
                role
            } else if is_replaced && !blockified {
                BoxType::Replaced(node_id)
            } else if let Some(role) = tag_role {
                role
            } else if blockified {
                BoxType::BlockNode(node_id)
            } else {
                match style.display {
                    Display::Block | Display::Flex | Display::Grid | Display::ListItem => {
                        BoxType::BlockNode(node_id)
                    }
                    _ => BoxType::InlineNode(node_id),
                }
            };

            // The element's own style handle (one deep clone per element box) and the
            // **shared** anonymous style for its generated descendants. The latter is
            // derived once per parent instead of once per text/anonymous box, which is
            // the dominant source of `ComputedStyle` clones on text-heavy pages.
            let own_style = style_cache.intern(style);
            // The shared anonymous style for generated descendants is built lazily:
            // most element boxes have only element children (no generated boxes), so
            // `ComputedStyle::initial() + inherit_from` is skipped for them.
            let mut child_style: Option<Arc<ComputedStyle>> = None;

            // Size the children buffer to the exact DOM child count instead of a fixed
            // `with_capacity(4)`: most elements have 1–2 children, so the old capacity
            // over-allocated (and grew to 8 on the fifth child) for every element.
            let child_count = {
                let mut count = 0usize;
                let mut child = node.first_child;
                while let Some(child_id) = child {
                    count += 1;
                    child = doc.node(child_id).next_sibling;
                }
                count
            };
            let mut raw_children = Vec::with_capacity(child_count);
            let mut curr = node.first_child;
            while let Some(child_id) = curr {
                let child_node = doc.node(child_id);
                let inherited_needed = match &child_node.kind {
                    NodeKind::Text(text) => !text.is_empty(),
                    NodeKind::Element(_) => styles.get(&child_id).is_none(),
                    _ => false,
                };
                if inherited_needed && child_style.is_none() {
                    child_style = Some(style_cache.anonymous_for(&own_style));
                }
                if let Some(child_box) = build_box_for_node(
                    doc,
                    child_id,
                    styles,
                    child_style.as_ref(),
                    replaced_sizes,
                    has_overflow_clip,
                    style_cache,
                ) {
                    raw_children.push(child_box);
                }
                curr = doc.node(child_id).next_sibling;
            }

            let needs_anonymous_style = match box_type {
                BoxType::BlockNode(_)
                | BoxType::InlineBlock(_)
                | BoxType::TableCell(_)
                | BoxType::TableCaption(_) => {
                    let mut has_block = false;
                    let mut has_inline = false;
                    for child in &raw_children {
                        if child.is_block() {
                            has_block = true;
                        } else {
                            has_inline = true;
                        }
                        if has_block && has_inline {
                            break;
                        }
                    }
                    has_block && has_inline
                }
                // Table fixups may need anonymous row-group/row/cell wrappers.
                BoxType::Table(_)
                | BoxType::InlineTable(_)
                | BoxType::TableRowGroup(_)
                | BoxType::TableRow(_) => true,
                _ => false,
            };
            if needs_anonymous_style && child_style.is_none() {
                child_style = Some(style_cache.anonymous_for(&own_style));
            }
            let children = match box_type {
                BoxType::BlockNode(_)
                | BoxType::InlineBlock(_)
                | BoxType::TableCell(_)
                | BoxType::TableCaption(_) => normalize_block_children(
                    raw_children,
                    child_style.as_ref(),
                    needs_anonymous_style,
                ),
                BoxType::Table(_) | BoxType::InlineTable(_) => {
                    table_fixup_children(raw_children, child_style.as_ref())
                }
                BoxType::TableRowGroup(_) => {
                    row_group_fixup_children(raw_children, child_style.as_ref())
                }
                BoxType::TableRow(_) => row_fixup_children(raw_children, child_style.as_ref()),
                // Inline/atomic/replaced/column boxes keep their children as-is.
                _ => raw_children,
            };

            let mut layout_box = LayoutBox::with_shared_style(box_type.clone(), Some(own_style));
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
            // Stacking-context flag (CSS 2.2 Appendix E), computed once at build time so
            // the document-wide finalize pass is only needed for `overflow` clipping.
            layout_box.establishes_stacking_context = style.opacity < 1.0
                || matches!(style.position, Position::Fixed)
                || (style.position != Position::Static && style.z_index.is_some());
            if !matches!(style.overflow, Overflow::Visible) {
                *has_overflow_clip = true;
            }
            if matches!(box_type, BoxType::TableCell(_)) {
                // Fast path: most cells have no attributes at all.
                if let Some(element) = node
                    .as_element()
                    .filter(|element| !element.attributes.is_empty())
                {
                    layout_box.colspan = element
                        .get_attribute("colspan")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 1000) as u16;
                    // HTML §table.attr.rowspan: 0 is treated as 1; upper bound 65534.
                    layout_box.rowspan = element
                        .get_attribute("rowspan")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .map_or(1, |value| value.clamp(1, 65534) as u16);
                }
            }
            if matches!(box_type, BoxType::Replaced(_)) {
                let from_api = replaced_sizes.and_then(|sizes| sizes.get(&node_id).copied());
                let element = node.as_element();
                let attribute_number = |name: &str| -> Option<f32> {
                    element
                        .and_then(|el| el.get_attribute(name))
                        .and_then(|value| value.trim().trim_end_matches("px").parse::<f32>().ok())
                        .filter(|value| value.is_finite() && *value >= 0.0)
                };
                let attribute_width = attribute_number("width");
                let attribute_height = attribute_number("height");
                layout_box.intrinsic_size = Some(from_api.unwrap_or_else(|| {
                    let (default_width, default_height) = replaced_default_size(&tag);
                    (
                        attribute_width.unwrap_or(default_width),
                        attribute_height.unwrap_or(default_height),
                    )
                }));
            }
            Some(layout_box)
        }
        NodeKind::Text(txt) => {
            // An empty text node generates no inline content.
            if txt.is_empty() {
                return None;
            }
            // Text boxes are formatting artifacts: they must only carry inherited
            // properties, never the parent's margins/borders/background (F-02). The
            // anonymous style is computed once per parent element and shared (`Arc`).
            Some(LayoutBox::with_shared_style(
                BoxType::TextNode(txt.clone()),
                inherited_style.cloned(),
            ))
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
    inherited_style: Option<&Arc<ComputedStyle>>,
    needs_wrapping: bool,
) -> Vec<LayoutBox> {
    // `needs_wrapping` is the `has_block && has_inline` scan already performed by the
    // caller for the anonymous-style decision, so pure-block/pure-inline children avoid a
    // second pair of scans over the child list.
    if !needs_wrapping {
        return children;
    }

    let anonymous_style = inherited_style.cloned();
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
    anonymous_style: &Option<Arc<ComputedStyle>>,
) {
    if run.is_empty() {
        return;
    }
    let significant = run.iter().any(|c| !is_collapsible_whitespace_box(c));
    if significant {
        let mut anon =
            LayoutBox::with_shared_style(BoxType::AnonymousBlock, anonymous_style.clone());
        anon.children = std::mem::take(run);
        normalized.push(anon);
    } else {
        run.clear();
    }
}

/// Default intrinsic size of a replaced element per HTML/CSS 2.2 conventions.
fn replaced_default_size(tag: &str) -> (f32, f32) {
    match tag {
        // CSS 2.2 §10.3.2: a replaced element with no intrinsic dimensions uses 300x150.
        "img" | "canvas" | "video" | "object" | "embed" | "iframe" => (300.0, 150.0),
        "input" | "select" => (20.0, 20.0),
        "textarea" => (160.0, 40.0),
        "audio" => (300.0, 30.0),
        _ => (300.0, 150.0),
    }
}

/// Wraps cells found directly inside a table box into an anonymous row (CSS 2.1
/// §17.2.1). Other stray children are wrapped into an anonymous row + cell.
fn table_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending_cells: Vec<LayoutBox> = Vec::new();
    let mut pending_other: Vec<LayoutBox> = Vec::new();
    for child in children {
        match child.box_type {
            BoxType::TableCell(_) => {
                flush_other_children(&mut pending_other, &mut out, inherited_style);
                pending_cells.push(child);
            }
            BoxType::TableRow(_)
            | BoxType::TableRowGroup(_)
            | BoxType::TableCaption(_)
            | BoxType::TableColumn(_) => {
                flush_cells(&mut pending_cells, &mut out, inherited_style);
                flush_other_children(&mut pending_other, &mut out, inherited_style);
                out.push(child);
            }
            _ => {
                flush_cells(&mut pending_cells, &mut out, inherited_style);
                pending_other.push(child);
            }
        }
    }
    flush_cells(&mut pending_cells, &mut out, inherited_style);
    flush_other_children(&mut pending_other, &mut out, inherited_style);
    out
}

fn flush_cells(
    cells: &mut Vec<LayoutBox>,
    out: &mut Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) {
    if cells.is_empty() {
        return;
    }
    let mut row =
        LayoutBox::with_shared_style(BoxType::AnonymousTableRow, inherited_style.cloned());
    row.children = std::mem::take(cells);
    out.push(row);
}

fn flush_other_children(
    children: &mut Vec<LayoutBox>,
    out: &mut Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) {
    if children.is_empty() {
        return;
    }
    let mut cell =
        LayoutBox::with_shared_style(BoxType::AnonymousTableCell, inherited_style.cloned());
    cell.children = std::mem::take(children);
    let mut row =
        LayoutBox::with_shared_style(BoxType::AnonymousTableRow, inherited_style.cloned());
    row.children.push(cell);
    out.push(row);
}

/// Children of a row group: rows are kept, stray cells/content are wrapped in an
/// anonymous row (CSS 2.1 §17.2.1).
fn row_group_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending: Vec<LayoutBox> = Vec::new();
    for child in children {
        if matches!(child.box_type, BoxType::TableRow(_)) {
            flush_cells(&mut pending, &mut out, inherited_style);
            out.push(child);
        } else {
            pending.push(child);
        }
    }
    flush_cells(&mut pending, &mut out, inherited_style);
    out
}

/// Children of a row: cells are kept, stray content is wrapped in an anonymous cell
/// (CSS 2.1 §17.2.1).
fn row_fixup_children(
    children: Vec<LayoutBox>,
    inherited_style: Option<&Arc<ComputedStyle>>,
) -> Vec<LayoutBox> {
    let mut out = Vec::new();
    let mut pending: Vec<LayoutBox> = Vec::new();
    for child in children {
        if matches!(child.box_type, BoxType::TableCell(_)) {
            flush_other_children(&mut pending, &mut out, inherited_style);
            out.push(child);
        } else {
            pending.push(child);
        }
    }
    // Stray content goes in an anonymous cell appended as a sibling row? CSS wraps it
    // in an anonymous cell inside the same row; flush as an inline cell.
    if !pending.is_empty() {
        let mut cell =
            LayoutBox::with_shared_style(BoxType::AnonymousTableCell, inherited_style.cloned());
        cell.children = pending;
        out.push(cell);
    }
    out
}
