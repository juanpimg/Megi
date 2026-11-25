use super::*;

use super::cache::replaced_default_size;
use super::fixups::{
    normalize_block_children, row_fixup_children, row_group_fixup_children, table_fixup_children,
};

#[path = "element_boxes.rs"]
mod element_boxes;
use element_boxes::{build_pseudo_box, is_html_br, select_control_content, select_label_box};

/// Recursively builds a layout box for a DOM node.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_box_for_node(
    doc: &Document,
    node_id: NodeId,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudo_styles: Option<&PseudoStyles>,
    // Inherited style for generated boxes: the anonymous style derived once from the
    // parent element (shared `Arc`), used directly by text and anonymous boxes.
    inherited_style: Option<&Arc<ComputedStyle>>,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
    has_overflow_clip: &mut bool,
    style_cache: &mut StyleCache,
    counters: &mut CounterScope,
    depth: usize,
) -> Option<LayoutBox> {
    if depth >= MAX_TREE_BUILD_DEPTH {
        return None;
    }
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
            let select_content = (tag == "select").then(|| select_control_content(doc, node_id));

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
            let is_explicit_table = matches!(
                style.display,
                Display::Table
                    | Display::InlineTable
                    | Display::TableRowGroup
                    | Display::TableHeaderGroup
                    | Display::TableFooterGroup
                    | Display::TableRow
                    | Display::TableCell
                    | Display::TableCaption
                    | Display::TableColumn
                    | Display::TableColumnGroup
            );
            let box_type = if is_replaced && !blockified && !is_explicit_table {
                BoxType::Replaced(node_id)
            } else if let Some(role) = display_role {
                role
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

            if pseudo_styles.is_some() {
                counters.apply(style, depth);
            }
            let allow_pseudos = !is_replaced && pseudo_styles.is_some();

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
                count + usize::from(allow_pseudos) * 2
            };
            let mut raw_children = Vec::with_capacity(child_count);
            if allow_pseudos
                && let Some(pseudos) = pseudo_styles
                && let Some(before_box) = build_pseudo_box(
                    doc,
                    node_id,
                    PseudoElement::Before,
                    pseudos,
                    style_cache,
                    counters,
                    has_overflow_clip,
                    depth,
                )
            {
                raw_children.push(before_box);
            }
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
                // CSS Display 3 §2.5: `display: contents` generates no box for the
                // element itself; its children are hoisted into this parent as if the
                // wrapper were removed. This lets them become direct grid/flex items
                // and inherit placement from the real container (e.g. MDN's
                // `.layout__content { display: contents }` inside a named-area grid).
                let child_is_contents = styles
                    .get(&child_id)
                    .is_some_and(|s| s.display == Display::Contents);
                if child_is_contents {
                    let mut grandchild = child_node.first_child;
                    while let Some(grandchild_id) = grandchild {
                        let gnode = doc.node(grandchild_id);
                        let g_inherited = match &gnode.kind {
                            NodeKind::Text(text) => !text.is_empty(),
                            NodeKind::Element(_) => styles.get(&grandchild_id).is_none(),
                            _ => false,
                        };
                        if g_inherited && child_style.is_none() {
                            child_style = Some(style_cache.anonymous_for(&own_style));
                        }
                        if let Some(boxed) = build_box_for_node(
                            doc,
                            grandchild_id,
                            styles,
                            pseudo_styles,
                            child_style.as_ref(),
                            replaced_sizes,
                            has_overflow_clip,
                            style_cache,
                            counters,
                            depth + 1,
                        ) {
                            raw_children.push(boxed);
                        }
                        grandchild = gnode.next_sibling;
                    }
                } else if let Some(child_box) = build_box_for_node(
                    doc,
                    child_id,
                    styles,
                    pseudo_styles,
                    child_style.as_ref(),
                    replaced_sizes,
                    has_overflow_clip,
                    style_cache,
                    counters,
                    depth + 1,
                ) {
                    raw_children.push(child_box);
                }
                curr = doc.node(child_id).next_sibling;
            }
            if allow_pseudos
                && let Some(pseudos) = pseudo_styles
                && let Some(after_box) = build_pseudo_box(
                    doc,
                    node_id,
                    PseudoElement::After,
                    pseudos,
                    style_cache,
                    counters,
                    has_overflow_clip,
                    depth,
                )
            {
                raw_children.push(after_box);
            }
            if pseudo_styles.is_some() {
                counters.pop_deeper_than(depth);
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

            // A replaced `<select>` keeps its atomic sizing, but its native label is
            // carried as a single generated text child for the paint pass. The
            // `<option>` boxes are not laid out: the control paints its own content.
            let select_text_box = select_content
                .as_ref()
                .filter(|_| matches!(box_type, BoxType::Replaced(_)))
                .map(|(label, _)| select_label_box(label, &child_style, &own_style, style_cache));
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
            layout_box.children = select_text_box.map_or(children, |text_box| vec![text_box]);
            // Stacking-context flag (CSS 2.2 Appendix E), computed once at build time so
            // the document-wide finalize pass is only needed for `overflow` clipping.
            layout_box.establishes_stacking_context = style.opacity.is_nan()
                || style.opacity < 1.0
                || matches!(style.position, Position::Fixed)
                || (style.position != Position::Static && style.z_index.is_some());
            if let Some(element) = node.as_element()
                && element.namespace == Namespace::Html
                && element.local_name.eq_ignore_ascii_case("dialog")
                && element.get_attribute("open").is_some()
                && let Some(modal_attr) = element.get_attribute("data-megi-modal")
            {
                layout_box.is_top_layer = true;
                layout_box.top_layer_order = modal_attr.trim().parse::<u32>().unwrap_or(0);
                layout_box.backdrop_color = style.backdrop_color;
                layout_box.establishes_stacking_context = true;
            }
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
                let from_api = replaced_sizes.and_then(|sizes| {
                    sizes.get(&node_id).copied().filter(|&(w, h)| {
                        (0.0..=10000.0).contains(&w) && (0.0..=10000.0).contains(&h)
                    })
                });
                let element = node.as_element();
                let attribute_number = |name: &str| -> Option<f32> {
                    element
                        .and_then(|el| el.get_attribute(name))
                        .and_then(|value| value.trim().trim_end_matches("px").parse::<f32>().ok())
                        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 10000.0)
                };
                let attribute_width = attribute_number("width");
                let attribute_height = attribute_number("height");
                layout_box.intrinsic_size = Some(from_api.unwrap_or_else(|| {
                    let (default_width, default_height) = replaced_default_size(&tag);
                    // HTML §15.4: <input size=N> sizes a text field to roughly N
                    // average character advances (~8px at the default font).
                    // google.com's search box uses size=57 (#2247).
                    let size_width = if tag == "input" {
                        attribute_number("size").map(|size| (size * 8.0).max(20.0))
                    } else {
                        None
                    };
                    let select_width = select_content.as_ref().map(|(_, width)| *width);
                    (
                        attribute_width
                            .or(size_width)
                            .or(select_width)
                            .unwrap_or(default_width),
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
