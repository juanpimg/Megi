//! `min-width: auto` / fit-content on flex items (CSS Flexbox 1 §4.5).
//!
//! An automatic minimum size is content-based: a flex item must not shrink below
//! its min-content width unless the word itself is wider than the line. The
//! previous long-word fix enabled arbitrary word splitting for every flex item,
//! which broke short labels ("Web", "CSS", "Theme") in narrow MDN sidebar pods.

use super::*;
use megi_css::WhiteSpace;

fn text_fragments(node: &LayoutBox, out: &mut Vec<String>) {
    if let BoxType::TextNode(text) = &node.box_type {
        out.push(text.clone());
    }
    for child in &node.children {
        text_fragments(child, out);
    }
}

fn zero_basis_item() -> ComputedStyle {
    let mut style = item_style(Length::Auto, Length::Auto);
    style.flex_grow = 1.0;
    style.flex_shrink = 1.0;
    style.flex_basis = Length::Px(0.0);
    style
}

/// Three short labels in a narrow flex row, as in an MDN sidebar pod.
fn pod_case(width: f32) -> (LayoutBox, Vec<NodeId>) {
    let (mut doc, mut styles, html) = setup();
    let row_id = add_element(
        &mut doc,
        html,
        &mut styles,
        flex_container_style(Length::Px(width), Length::Px(80.0)),
    );
    let mut ids = Vec::new();
    for label in ["Web", "CSS", "Theme"] {
        let id = add_element(&mut doc, row_id, &mut styles, zero_basis_item());
        doc.append_text(id, label);
        ids.push(id);
    }
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1024.0, 768.0))
        .expect("layout must succeed");
    (root, ids)
}

#[test]
fn short_flex_item_words_are_not_split() {
    // CSS Sizing 3 §5: min-content of a word is its full advance (8px/char here).
    // The 100px row can hold 24+24+40 = 88px of min-content, so no label may break.
    let (root, ids) = pod_case(100.0);
    for (index, label) in ["Web", "CSS", "Theme"].into_iter().enumerate() {
        let mut fragments = Vec::new();
        text_fragments(find_box(&root, ids[index]), &mut fragments);
        assert!(
            fragments.iter().any(|text| text == label),
            "{label} must stay one unbroken word, got fragments {fragments:?}"
        );
        assert_eq!(fragments.len(), 1, "{label} produced multiple text runs");
    }
}

#[test]
fn fit_content_flex_item_uses_intrinsic_width() {
    // A `width: fit-content` flex item resolves its base size from its inline
    // content (CSS Sizing 3 §5) instead of treating the value as `auto`.
    let (mut doc, mut styles, html) = setup();
    let row_id = add_element(
        &mut doc,
        html,
        &mut styles,
        flex_container_style(Length::Px(400.0), Length::Px(80.0)),
    );
    let id = add_element(&mut doc, row_id, &mut styles, zero_basis_item());
    let mut fit_style = zero_basis_item();
    fit_style.flex_grow = 0.0; // keep the intrinsic size; no free space is claimed
    fit_style.width_fit_content = true;
    fit_style.white_space = WhiteSpace::Normal;
    styles.insert(id, fit_style);
    doc.append_text(id, "Web");
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1024.0, 768.0))
        .expect("layout must succeed");
    let width = find_box(&root, id).dimensions.content.width;
    assert!(
        (width - 24.0).abs() < 0.001,
        "fit-content flex item must shrink to its 24px word, got {width}"
    );
}
