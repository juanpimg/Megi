//! Regression: an auto-sized flex item's shrink-to-fit width must be a
//! content-box size. The intrinsic contribution includes the item's own edges;
//! adding them again when forming the flex base size double-counts horizontal
//! padding and wraps rows that fit (CSS Flexbox 1 §5.3.3).

use super::*;

#[test]
fn flex_item_padding_is_not_double_counted_in_base_size() {
    let container = {
        let mut style = flex_container_style(Length::Px(951.0), Length::Auto);
        style.flex_wrap = FlexWrap::Wrap;
        style
    };
    let mut item = item_style(Length::Auto, Length::Auto);
    item.padding.left = Length::Px(24.0);
    item.padding.right = Length::Px(24.0);
    item.font_size = 16.0;

    let (mut doc, mut styles, html) = setup();
    let ul = add_element(&mut doc, html, &mut styles, container);
    let mut ids = Vec::new();
    for _ in 0..8 {
        let li = add_element(&mut doc, ul, &mut styles, item.clone());
        // DefaultTextMeasurer: 7 chars * 0.5 * 16px = 56px of text.
        let text = doc.create_text_node("Install");
        assert!(doc.append_child(li, text));
        ids.push(li);
    }
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1000.0, 700.0)).expect("layout result");

    let boxes: Vec<_> = ids.iter().map(|id| find_box(&root, *id)).collect();
    for (index, li) in boxes.iter().enumerate() {
        assert!(
            (li.dimensions.content.width - 56.0).abs() < 0.5,
            "item {index} content width was {} (expected 56: padding counted once)",
            li.dimensions.content.width
        );
    }
    let first_y = boxes[0].dimensions.content.y;
    assert!(
        boxes
            .iter()
            .all(|li| (li.dimensions.content.y - first_y).abs() < 0.5),
        "8 items of 56 + 24 + 24 = 104px fit in 951px; they must not wrap"
    );
}
