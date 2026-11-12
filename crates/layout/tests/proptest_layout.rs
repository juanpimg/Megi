//! Property-based tests for `layout` box-tree construction and BFC/flex/grid/table/inline layout (#961).

use std::collections::HashMap;

use css::{ComputedStyle, Display, Position, resolve_document_styles_with_viewport};
use dom::{Attribute, Document, Namespace, NodeId};
use layout::{BoxType, LayoutBox, Rect, layout_document, layout_document_with_replaced_sizes};
use proptest::prelude::*;

fn assert_finite_non_negative_box_tree(b: &LayoutBox) {
    let mut stack = vec![b];
    while let Some(node) = stack.pop() {
        let d = &node.dimensions;
        for rect in [d.content, d.padding_box(), d.border_box(), d.margin_box()] {
            assert!(rect.x.is_finite(), "non-finite x: {rect:?}");
            assert!(rect.y.is_finite(), "non-finite y: {rect:?}");
            assert!(
                rect.width.is_finite() && rect.width >= 0.0,
                "bad width: {rect:?}"
            );
            assert!(
                rect.height.is_finite() && rect.height >= 0.0,
                "bad height: {rect:?}"
            );
        }
        for child in &node.children {
            stack.push(child);
        }
    }
}

fn contains_node(b: &LayoutBox, target: NodeId) -> bool {
    let mut stack = vec![b];
    while let Some(node) = stack.pop() {
        match node.box_type {
            BoxType::BlockNode(id) | BoxType::InlineNode(id) | BoxType::Replaced(id)
                if id == target =>
            {
                return true;
            }
            _ => {}
        }
        for child in &node.children {
            stack.push(child);
        }
    }
    false
}

fn build_random_dom(specs: &[(u8, bool)]) -> (Document, Vec<NodeId>) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);

    let mut ids = Vec::new();
    let mut parents = vec![body];
    for (idx, (tag_idx, has_text)) in specs.iter().enumerate() {
        let tag = match tag_idx % 6 {
            0 => "div",
            1 => "span",
            2 => "p",
            3 => "table",
            4 => "img",
            _ => "section",
        };
        let el = doc.create_element(
            Namespace::Html,
            tag,
            vec![Attribute::new("id", format!("n{idx}"))],
        );
        let parent = parents[idx % parents.len()];
        doc.append_child(parent, el);
        if *has_text && tag != "img" {
            let txt = doc.create_text_node("hello world layout");
            doc.append_child(el, txt);
        }
        ids.push(el);
        if tag != "img" {
            parents.push(el);
        }
    }
    (doc, ids)
}

fn display_strategy() -> impl Strategy<Value = Display> {
    prop_oneof![
        Just(Display::Block),
        Just(Display::Inline),
        Just(Display::InlineBlock),
        Just(Display::Flex),
        Just(Display::Grid),
        Just(Display::Table),
        Just(Display::None),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_dom_and_css_layout_produces_finite_geometry(
        specs in prop::collection::vec((any::<u8>(), any::<bool>()), 1..25),
        vw in 1.0f32..2000.0,
        vh in 1.0f32..2000.0,
    ) {
        let (doc, _) = build_random_dom(&specs);
        let styles = resolve_document_styles_with_viewport(&doc, None, vw, vh);
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, vw, vh)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn layout_is_deterministic(
        specs in prop::collection::vec((any::<u8>(), any::<bool>()), 1..20),
        vw in 10.0f32..1600.0,
        vh in 10.0f32..1200.0,
    ) {
        let (doc, _) = build_random_dom(&specs);
        let styles = resolve_document_styles_with_viewport(&doc, None, vw, vh);
        let vp = Rect::new(0.0, 0.0, vw, vh);
        let a = layout_document(&doc, &styles, vp);
        let b = layout_document(&doc, &styles, vp);
        prop_assert_eq!(a.is_some(), b.is_some());
        if let (Some(la), Some(lb)) = (a, b) {
            prop_assert_eq!(la.dimensions.content, lb.dimensions.content);
            prop_assert_eq!(la.children.len(), lb.children.len());
        }
    }

    #[test]
    fn extreme_viewports_never_panic_or_produce_nan(
        vw in prop_oneof![Just(0.0f32), Just(0.5), Just(1.0), Just(100_000.0), 0.0f32..5000.0],
        vh in prop_oneof![Just(0.0f32), Just(0.5), Just(1.0), Just(100_000.0), 0.0f32..5000.0],
    ) {
        let (doc, _) = build_random_dom(&[(0, true), (1, true), (2, true)]);
        let styles = resolve_document_styles_with_viewport(&doc, None, vw, vh);
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, vw, vh)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn mixed_display_modes_never_panic_or_produce_nan(
        displays in prop::collection::vec(display_strategy(), 1..16),
    ) {
        let specs: Vec<(u8, bool)> = (0..displays.len()).map(|i| ((i % 3) as u8, true)).collect();
        let (doc, ids) = build_random_dom(&specs);
        let mut styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
        for (id, disp) in ids.iter().zip(displays.iter()) {
            styles.entry(*id).or_insert_with(ComputedStyle::initial).display = *disp;
        }
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn display_none_nodes_are_excluded_from_box_tree(hide_idx in 0usize..8) {
        let specs = vec![(0, true); 8];
        let (doc, ids) = build_random_dom(&specs);
        let mut styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
        let hidden = ids[hide_idx];
        styles.entry(hidden).or_insert_with(ComputedStyle::initial).display = Display::None;
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            prop_assert!(!contains_node(&root, hidden));
        }
    }

    #[test]
    fn deep_block_nesting_never_overflows_stack(depth in 1usize..250) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        let mut cur = body;
        for _ in 0..depth {
            let div = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(cur, div);
            cur = div;
        }
        let txt = doc.create_text_node("deep");
        doc.append_child(cur, txt);
        let styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn replaced_image_intrinsic_sizes_produce_finite_geometry(
        iw in 0.0f32..4000.0,
        ih in 0.0f32..4000.0,
    ) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let img = doc.create_element(Namespace::Html, "img", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, img);
        let styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
        let mut replaced = HashMap::new();
        replaced.insert(img, (iw, ih));
        if let Some(root) = layout_document_with_replaced_sizes(
            &doc,
            &styles,
            Rect::new(0.0, 0.0, 800.0, 600.0),
            &replaced,
        ) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn border_box_and_content_box_never_yield_negative_content_size(
        w in 0.0f32..500.0,
        h in 0.0f32..500.0,
        pad in 0.0f32..300.0,
        border in 0.0f32..100.0,
        is_border_box in any::<bool>(),
    ) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", "b")]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, div);
        let bs = if is_border_box { "border-box" } else { "content-box" };
        let css = format!(
            "#b {{ box-sizing: {bs}; width: {w}px; height: {h}px; padding: {pad}px; border: {border}px solid black; }}"
        );
        let styles = resolve_document_styles_with_viewport(&doc, Some(&css), 800.0, 600.0);
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn positioned_offsets_never_panic_or_produce_nan(
        pos in prop_oneof![
            Just(Position::Static),
            Just(Position::Relative),
            Just(Position::Absolute),
            Just(Position::Fixed),
        ],
        top in -500.0f32..500.0,
        left in -500.0f32..500.0,
    ) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", "p")]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, div);
        let css = format!("#p {{ width: 50px; height: 50px; top: {top}px; left: {left}px; }}");
        let mut styles = resolve_document_styles_with_viewport(&doc, Some(&css), 800.0, 600.0);
        styles.entry(div).or_insert_with(ComputedStyle::initial).position = pos;
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }

    #[test]
    fn flex_and_grid_containers_with_arbitrary_children_are_finite(
        is_grid in any::<bool>(),
        count in 0usize..12,
    ) {
        let specs = vec![(0, true); count];
        let (doc, ids) = build_random_dom(&specs);
        let mut styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
        if let Some(&first) = ids.first() {
            styles.entry(first).or_insert_with(ComputedStyle::initial).display = if is_grid {
                Display::Grid
            } else {
                Display::Flex
            };
        }
        if let Some(root) = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)) {
            assert_finite_non_negative_box_tree(&root);
        }
    }
}
