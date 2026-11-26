//! Regression: an auto-sized flex item's shrink-to-fit width must be a
//! content-box size. The intrinsic contribution includes the item's own edges;
//! adding them again when forming the flex base size double-counts horizontal
//! padding and wraps rows that fit (CSS Flexbox 1 §5.3.3).

use super::*;
use megi_layout::{FontMetrics, TextMeasurer, layout_document_with_measurer};

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

/// Regression: an auto-width flex item that is itself a wrapping row must take
/// its **max-content** main size as the flex base size. Shrink-to-fit clamping
/// it to the parent's available width rewraps its children (the rust-lang.org
/// nav went 7+1 instead of one row). CSS Flexbox 1 §9.2 step 3D; CSS Sizing 3 §5.
#[test]
fn auto_width_nested_wrapping_flex_row_uses_max_content_base_size() {
    let mut nav_style = flex_container_style(Length::Px(968.0), Length::Auto);
    nav_style.flex_wrap = FlexWrap::Wrap;
    let mut ul_style = flex_container_style(Length::Auto, Length::Auto);
    ul_style.flex_grow = 0.0;
    ul_style.flex_shrink = 0.0; // `flex: none` in the real stylesheet
    ul_style.flex_wrap = FlexWrap::Wrap;
    ul_style.padding.left = Length::Px(24.0);
    ul_style.padding.right = Length::Px(24.0);
    let mut item = item_style(Length::Auto, Length::Auto);
    item.padding.left = Length::Px(24.0);
    item.padding.right = Length::Px(24.0);
    // DefaultTextMeasurer advances 0.5em per char. At 24px the eight labels
    // total 57 * 12 = 684px of text plus 8 * 48px padding = 1068px, which is
    // wider than the 968px nav: only max-content sizing keeps one row.
    item.font_size = 24.0;
    let labels = [
        "Install",
        "Learn",
        "Playground",
        "Tools",
        "Governance",
        "Funding",
        "Community",
        "Blog",
    ];

    let (mut doc, mut styles, html) = setup();
    let nav = add_element(&mut doc, html, &mut styles, nav_style);
    let ul = add_element(&mut doc, nav, &mut styles, ul_style);
    let mut ids = Vec::new();
    for label in labels {
        let li = add_element(&mut doc, ul, &mut styles, item.clone());
        let text = doc.create_text_node(label);
        assert!(doc.append_child(li, text));
        ids.push(li);
    }
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1000.0, 700.0)).expect("layout result");

    let ul_width = find_box(&root, ul).dimensions.content.width;
    assert!(
        (ul_width - 1068.0).abs() < 1.0,
        "auto flex base must be max-content 1068px (CSS Flexbox 1 §9.2 step 3D), got {ul_width}"
    );
    let boxes: Vec<_> = ids.iter().map(|id| find_box(&root, *id)).collect();
    let first_y = boxes[0].dimensions.content.y;
    assert!(
        boxes
            .iter()
            .all(|li| (li.dimensions.content.y - first_y).abs() < 0.5),
        "all 8 items must stay on one line; ys = {:?}",
        boxes
            .iter()
            .map(|li| li.dimensions.content.y)
            .collect::<Vec<_>>()
    );
}

/// Regression: real font advances are fractional. The flex base size of each
/// item rounds its content box up (CSS Flexbox 1 §9.2 step 3D), so the container's
/// max-content size must use the same rounded contributions (CSS Sizing 3 §5).
/// Using the raw intrinsic width left the nav 4px short and wrapped "Blog".
#[test]
fn fractional_advances_keep_wrapping_flex_row_on_one_line() {
    struct FractionalMeasurer;
    impl TextMeasurer for FractionalMeasurer {
        fn measure_text(&self, text: &str, _family: &str, _size: f32) -> f32 {
            text.chars().count() as f32 * 9.65
        }
        fn font_metrics(&self, _family: &str, size: f32, line_height: Option<f32>) -> FontMetrics {
            FontMetrics::new(size, line_height)
        }
    }

    let mut nav_style = flex_container_style(Length::Px(976.0), Length::Auto);
    nav_style.flex_wrap = FlexWrap::Wrap;
    let mut ul_style = flex_container_style(Length::Auto, Length::Auto);
    ul_style.flex_grow = 0.0;
    ul_style.flex_shrink = 0.0;
    ul_style.flex_wrap = FlexWrap::Wrap;
    ul_style.padding.left = Length::Px(24.0);
    ul_style.padding.right = Length::Px(24.0);
    let mut item = item_style(Length::Auto, Length::Auto);
    item.padding.left = Length::Px(24.0);
    item.padding.right = Length::Px(24.0);
    let labels = [
        "Install",
        "Learn",
        "Playground",
        "Tools",
        "Governance",
        "Funding",
        "Community",
        "Blog",
    ];

    let (mut doc, mut styles, html) = setup();
    let nav = add_element(&mut doc, html, &mut styles, nav_style);
    let ul = add_element(&mut doc, nav, &mut styles, ul_style);
    let mut ids = Vec::new();
    for label in labels {
        let li = add_element(&mut doc, ul, &mut styles, item.clone());
        let text = doc.create_text_node(label);
        assert!(doc.append_child(li, text));
        ids.push(li);
    }
    let root = layout_document_with_measurer(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 1000.0, 700.0),
        &FractionalMeasurer,
    )
    .expect("layout result");

    let ul_width = find_box(&root, ul).dimensions.content.width;
    // Raw text sum is 550.05px; per-item ceil makes the row need 938px including
    // padding. The container must reserve that same 938px, not 934.05px.
    assert!(
        (ul_width - 938.0).abs() < 1.0,
        "max-content must include rounded item bases: expected 938, got {ul_width}"
    );
    let boxes: Vec<_> = ids.iter().map(|id| find_box(&root, *id)).collect();
    let first_y = boxes[0].dimensions.content.y;
    assert!(
        boxes
            .iter()
            .all(|li| (li.dimensions.content.y - first_y).abs() < 0.5),
        "all 8 items must stay on one line with fractional advances; ys = {:?}",
        boxes
            .iter()
            .map(|li| li.dimensions.content.y)
            .collect::<Vec<_>>()
    );
}
