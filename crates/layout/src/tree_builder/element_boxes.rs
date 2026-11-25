//! Helpers that build the non-recursive part of element boxes: pseudo-element
//! boxes and the generated label box used by the native `<select>` control.
//! Kept separate from `box_for_node.rs` to respect the module size budget.

// Declared from `box_for_node.rs` with an explicit path; `super::super` is the
// `tree_builder` module that owns the shared layout imports.
use super::super::*;

use super::super::cache::{CounterScope, StyleCache};

/// Returns `true` if the node is an HTML `<br>` element.
pub(super) fn is_html_br(node: &megi_core::Node) -> bool {
    node.as_element().is_some_and(|el| {
        el.namespace == Namespace::Html && el.local_name.eq_ignore_ascii_case("br")
    })
}

/// WHATWG HTML §13.2.6 (`select`) / §15.4 (UA stylesheet): a select's
/// intrinsic width follows its longest option label. Returns the selected
/// option's text (first option when none is marked `selected`) and that width.
/// The 8px-per-character estimate matches the `<input size=N>` rule (#2247);
/// 18px are reserved inside the content box for the native arrow.
pub(super) fn select_control_content(doc: &Document, select_id: NodeId) -> (String, f32) {
    let mut longest = 0usize;
    let mut first = None;
    let mut selected = None;
    let mut option_id = doc.node(select_id).first_child;
    while let Some(id) = option_id {
        let option = doc.node(id);
        let is_option = option.as_element().is_some_and(|el| {
            el.namespace == Namespace::Html && el.local_name.eq_ignore_ascii_case("option")
        });
        if is_option {
            let mut label = String::new();
            let mut child_id = option.first_child;
            while let Some(child) = child_id {
                if let NodeKind::Text(text) = &doc.node(child).kind {
                    label.push_str(text);
                }
                child_id = doc.node(child).next_sibling;
            }
            if label.is_empty()
                && let Some(value) = option.as_element().and_then(|el| el.get_attribute("label"))
            {
                label.push_str(value);
            }
            longest = longest.max(label.chars().count());
            if first.is_none() {
                first = Some(label.clone());
            }
            let is_selected = option
                .as_element()
                .is_some_and(|el| el.get_attribute("selected").is_some());
            if is_selected && selected.is_none() {
                selected = Some(label);
            }
        }
        option_id = option.next_sibling;
    }
    let text = selected.or(first).unwrap_or_default();
    (text, longest as f32 * 8.0 + 18.0)
}

/// Wraps the selected option label in a generated text box. The paint pass uses
/// this box as the native-content marker for a replaced `<select>`.
pub(super) fn select_label_box(
    label: &str,
    child_style: &Option<Arc<ComputedStyle>>,
    own_style: &Arc<ComputedStyle>,
    style_cache: &mut StyleCache,
) -> LayoutBox {
    let text_style = child_style
        .clone()
        .unwrap_or_else(|| style_cache.anonymous_for(own_style));
    LayoutBox::with_shared_style(BoxType::TextNode(label.to_owned()), Some(text_style))
}

/// Builds a layout box for a `::before` or `::after` pseudo-element when generated.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_pseudo_box(
    doc: &Document,
    originating: NodeId,
    pseudo: PseudoElement,
    pseudo_styles: &PseudoStyles,
    style_cache: &mut StyleCache,
    counters: &mut CounterScope,
    has_overflow_clip: &mut bool,
    depth: usize,
) -> Option<LayoutBox> {
    let style = pseudo_styles.get(&(originating, pseudo))?;
    if style.display == Display::None || !style.content.generates_box() {
        return None;
    }
    counters.apply(style, depth);

    let blockified = style.float != Float::None
        || matches!(style.position, Position::Absolute | Position::Fixed);
    let is_replaced_pseudo = style.content.image_url().is_some();

    let box_type = if is_replaced_pseudo {
        BoxType::Replaced(originating)
    } else {
        match style.display {
            Display::Table => BoxType::Table(originating),
            Display::InlineTable => BoxType::InlineTable(originating),
            Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => {
                BoxType::TableRowGroup(originating)
            }
            Display::TableRow => BoxType::TableRow(originating),
            Display::TableCell => BoxType::TableCell(originating),
            Display::TableCaption => BoxType::TableCaption(originating),
            Display::TableColumn | Display::TableColumnGroup => BoxType::TableColumn(originating),
            Display::InlineBlock => BoxType::InlineBlock(originating),
            _ if blockified => BoxType::BlockNode(originating),
            Display::Block | Display::Flex | Display::Grid | Display::ListItem => {
                BoxType::BlockNode(originating)
            }
            _ => BoxType::InlineNode(originating),
        }
    };

    let own_style = style_cache.intern(style);
    let mut layout_box = LayoutBox::with_shared_style(box_type, Some(Arc::clone(&own_style)));
    layout_box.is_pseudo = true;
    layout_box.position = style.position;
    layout_box.z_index = if style.position == Position::Static {
        0
    } else {
        style.z_index.unwrap_or(0)
    };
    layout_box.opacity = style.opacity;
    layout_box.is_float = style.float != Float::None;
    layout_box.float_side = style.float;
    layout_box.establishes_stacking_context = style.opacity.is_nan()
        || style.opacity < 1.0
        || matches!(style.position, Position::Fixed)
        || (style.position != Position::Static && style.z_index.is_some());
    if !matches!(style.overflow, Overflow::Visible) {
        *has_overflow_clip = true;
    }

    if is_replaced_pseudo {
        layout_box.intrinsic_size = Some((300.0, 150.0));
    } else {
        let needs_counters = matches!(
            &style.content,
            megi_css::Content::Items(items) if items.iter().any(|it| matches!(it, ContentItem::Counter(_)))
        );
        let counter_map = if needs_counters {
            counters.snapshot()
        } else {
            HashMap::new()
        };
        if let Some(text) = style.content.resolve_text(doc, originating, &counter_map)
            && !text.is_empty()
        {
            let text_style = style_cache.anonymous_for(&own_style);
            layout_box.children.push(LayoutBox::with_shared_style(
                BoxType::TextNode(text),
                Some(text_style),
            ));
        }
    }

    Some(layout_box)
}
