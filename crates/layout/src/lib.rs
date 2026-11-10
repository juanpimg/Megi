//! # Megi Layout Engine (`layout`)
//!
//! Box Tree construction, Block Formatting Context (BFC), and 2D box geometry layout
//! per CSS 2.2 §8–10 and CSS Display Module Level 3.

#![forbid(unsafe_code)]

pub mod block;
pub mod box_tree;
mod flex;
pub mod geometry;
pub mod inline;
pub mod table;
pub mod text;
pub mod tree_builder;

use std::collections::HashMap;

use css::{ComputedStyle, Overflow, Position};
use dom::{Document, NodeId};

use block::layout_block_with_viewport;
pub use block::{layout_block, layout_block_with_measurer};
pub use box_tree::{BoxType, LayoutBox};
pub use geometry::{Dimensions, EdgeSizes, Rect};
pub use inline::{
    DefaultTextMeasurer, FontMetrics, TextMeasurer, layout_inline_formatting_context,
    layout_inline_formatting_context_with_measurer,
};
pub use text::FontTextMeasurer;
use tree_builder::build_layout_tree_internal;
pub use tree_builder::{build_layout_tree, build_layout_tree_with_replaced_sizes};

/// Completely lays out a DOM document against a viewport rectangle.
///
/// 1. Constructs the CSS Box Tree (handling `display: none` and anonymous block wrappers).
/// 2. Performs Block Formatting Context layout from the root containing block.
///
/// The initial containing block (ICB) has the dimensions of the viewport (CSS 2.2
/// §10.1), so the root element's percentage heights and the reported page height have a
/// meaningful reference.
///
/// Returns the root [`LayoutBox`] with populated geometric coordinates and dimensions.
pub fn layout_document(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
) -> Option<LayoutBox> {
    // Backwards-compatible deterministic measurement (CSS ratio fallback). The
    // production render pipeline must use `layout_document_with_measurer` with
    // `FontTextMeasurer` so that real OpenType metrics drive line boxes.
    layout_document_with_measurer_and_replaced_sizes(
        doc,
        styles,
        viewport,
        &DefaultTextMeasurer,
        None,
    )
}

/// Lays out a document with explicit intrinsic sizes for replaced elements
/// (`<img>`, `<input>`, …), keyed by DOM node, using the deterministic measurer.
///
/// This is the additive API used by the render pipeline to inject decoded image
/// dimensions: `layout_document(doc, styles, viewport)` keeps its signature and falls
/// back to CSS 2.2/HTML per-kind defaults (e.g. `<img>` 300×150).
pub fn layout_document_with_replaced_sizes(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
    replaced_sizes: &HashMap<NodeId, (f32, f32)>,
) -> Option<LayoutBox> {
    layout_document_with_measurer_and_replaced_sizes(
        doc,
        styles,
        viewport,
        &DefaultTextMeasurer,
        Some(replaced_sizes),
    )
}

/// Lays out a document with an explicit [`TextMeasurer`].
///
/// This is the entry point used by the Phase 4 `HTML → PNG` pipeline:
/// `layout_document_with_measurer(doc, styles, viewport, &FontTextMeasurer::bundled()?)`
/// produces line boxes, baselines and glyph advances from real font metrics.
pub fn layout_document_with_measurer(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
    measurer: &dyn TextMeasurer,
) -> Option<LayoutBox> {
    layout_document_with_measurer_and_replaced_sizes(doc, styles, viewport, measurer, None)
}

/// Full pipeline entry point: explicit measurer plus optional intrinsic replaced sizes.
pub fn layout_document_with_measurer_and_replaced_sizes(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
    measurer: &dyn TextMeasurer,
    replaced_sizes: Option<&HashMap<NodeId, (f32, f32)>>,
) -> Option<LayoutBox> {
    let (mut root_box, has_overflow_clip) =
        build_layout_tree_internal(doc, styles, replaced_sizes)?;

    let initial_containing_block = Dimensions {
        content: Rect::new(viewport.x, viewport.y, viewport.width, viewport.height),
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };

    layout_block_with_viewport(&mut root_box, &initial_containing_block, measurer, viewport);
    if has_overflow_clip {
        finalize_layout_metadata(&mut root_box, None, true);
    } else {
        // No box clips: the only missing metadata is the root stacking context.
        root_box.establishes_stacking_context = true;
    }
    Some(root_box)
}

/// Fills in the derived layout metadata consumed by paint (WP-4.6/4.10 contract):
///
/// * `establishes_stacking_context` per CSS 2.2 Appendix E (root element, `fixed`
///   boxes, positioned boxes with a non-`auto` `z-index`, and `opacity < 1`);
/// * `clip`: absolute-coordinate clip rectangle inherited from ancestors and narrowed
///   by every box whose `overflow` is not `visible` (CSS 2.2 §11.1). The clipping box
///   itself carries its own padding box.
fn finalize_layout_metadata(node: &mut LayoutBox, inherited_clip: Option<Rect>, is_root: bool) {
    let mut effective_clip = inherited_clip;
    if let Some(style) = node.style.as_ref()
        && !matches!(style.overflow, Overflow::Visible)
    {
        let padding_box = node.dimensions.padding_box();
        effective_clip = Some(match inherited_clip {
            Some(parent_clip) => parent_clip.intersect(padding_box).unwrap_or(Rect::new(
                padding_box.x.max(parent_clip.x),
                padding_box.y.max(parent_clip.y),
                0.0,
                0.0,
            )),
            None => padding_box,
        });
    }
    node.clip = effective_clip;

    node.establishes_stacking_context = is_root
        || node.style.as_ref().is_some_and(|style| {
            style.opacity < 1.0
                || matches!(style.position, Position::Fixed)
                || (style.position != Position::Static && style.z_index.is_some())
        });

    for child in &mut node.children {
        finalize_layout_metadata(child, effective_clip, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use css::{Display, Length};
    use dom::{Document, Namespace};

    fn make_style(display: Display, width: Length, height: Length) -> ComputedStyle {
        let mut s = ComputedStyle::initial();
        s.display = display;
        s.width = width;
        s.height = height;
        s
    }

    #[test]
    fn test_layout_box_tree_generation() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let hidden = doc.create_element(Namespace::Html, "script", vec![]);

        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, div);
        doc.append_child(body, hidden);

        let mut styles = HashMap::new();
        styles.insert(html, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(body, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(
            div,
            make_style(Display::Block, Length::Px(100.0), Length::Px(50.0)),
        );
        styles.insert(
            hidden,
            make_style(Display::None, Length::Auto, Length::Auto),
        );

        let box_tree = build_layout_tree(&doc, &styles).expect("box tree root");
        assert_eq!(box_tree.box_type, BoxType::BlockNode(html));

        let body_box = &box_tree.children[0];
        assert_eq!(body_box.box_type, BoxType::BlockNode(body));

        // The hidden element (display: none) must generate NO box
        assert_eq!(body_box.children.len(), 1);
        assert_eq!(body_box.children[0].box_type, BoxType::BlockNode(div));
    }

    #[test]
    fn test_anonymous_block_box_generation() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);

        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, span);
        doc.append_child(body, div);

        let mut styles = HashMap::new();
        styles.insert(html, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(body, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(
            span,
            make_style(Display::Inline, Length::Auto, Length::Auto),
        );
        styles.insert(div, make_style(Display::Block, Length::Auto, Length::Auto));

        let box_tree = build_layout_tree(&doc, &styles).expect("box tree");
        let body_box = &box_tree.children[0];

        // Mixed inline (span) and block (div) children inside a block (body) must wrap span in AnonymousBlock
        assert_eq!(body_box.children.len(), 2);
        assert_eq!(body_box.children[0].box_type, BoxType::AnonymousBlock);
        assert_eq!(
            body_box.children[0].children[0].box_type,
            BoxType::InlineNode(span)
        );
        assert_eq!(body_box.children[1].box_type, BoxType::BlockNode(div));
    }

    #[test]
    fn test_block_formatting_context_width_and_auto_margins() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, div);

        let mut div_style = make_style(Display::Block, Length::Px(400.0), Length::Px(200.0));
        // Auto margins center horizontally: 800 - 400 = 400 underflow -> 200px each side
        div_style.margin.left = Length::Auto;
        div_style.margin.right = Length::Auto;

        let mut styles = HashMap::new();
        styles.insert(html, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(div, div_style);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let layout_tree = layout_document(&doc, &styles, viewport).expect("laid out document");

        let div_box = &layout_tree.children[0];
        assert_eq!(div_box.dimensions.content.width, 400.0);
        assert_eq!(div_box.dimensions.margin.left, 200.0);
        assert_eq!(div_box.dimensions.margin.right, 200.0);
        assert_eq!(div_box.dimensions.content.x, 200.0);
    }

    #[test]
    fn test_block_vertical_stacking_and_auto_height() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let b1 = doc.create_element(Namespace::Html, "div", vec![]);
        let b2 = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, b1);
        doc.append_child(html, b2);

        let mut s1 = make_style(Display::Block, Length::Auto, Length::Px(50.0));
        s1.margin.bottom = Length::Px(10.0);
        let s2 = make_style(Display::Block, Length::Auto, Length::Px(80.0));

        let mut styles = HashMap::new();
        styles.insert(html, make_style(Display::Block, Length::Auto, Length::Auto));
        styles.insert(b1, s1);
        styles.insert(b2, s2);

        let viewport = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let layout_tree = layout_document(&doc, &styles, viewport).expect("laid out document");

        let box1 = &layout_tree.children[0];
        let box2 = &layout_tree.children[1];

        assert_eq!(box1.dimensions.content.y, 0.0);
        assert_eq!(box1.dimensions.content.height, 50.0);

        // box2 stacks below box1: y = 50 (height) + 10 (margin-bottom) = 60.0
        assert_eq!(box2.dimensions.content.y, 60.0);
        assert_eq!(box2.dimensions.content.height, 80.0);

        // html auto height = 50 + 10 + 80 = 140.0
        assert_eq!(layout_tree.dimensions.content.height, 140.0);
    }
}
