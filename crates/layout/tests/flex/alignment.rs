use super::*;

#[test]
fn test_baseline_alignment_uses_line_boxes() {
    // Items with text at different font sizes must be aligned so their first-line
    // baselines coincide (#1381).
    fn first_line_baseline(layout_box: &LayoutBox) -> Option<f32> {
        if matches!(layout_box.box_type, BoxType::LineBox) {
            return Some(layout_box.dimensions.content.y + layout_box.baseline);
        }
        for child in &layout_box.children {
            if let Some(baseline) = first_line_baseline(child) {
                return Some(baseline);
            }
        }
        None
    }

    let (mut doc, mut styles, html) = setup();
    let mut container = flex_container_style(Length::Px(300.0), Length::Px(80.0));
    container.align_items = AlignItems::Baseline;
    let container_id = add_element(&mut doc, html, &mut styles, container);

    let mut first_style = item_style(Length::Px(50.0), Length::Auto);
    first_style.font_size = 10.0;
    first_style.line_height = Some(12.0);
    first_style.font_family = "monospace".to_string();
    let first = add_element(&mut doc, container_id, &mut styles, first_style);
    doc.append_text(first, "a");

    let mut second_style = item_style(Length::Px(50.0), Length::Auto);
    second_style.font_size = 30.0;
    second_style.line_height = Some(36.0);
    second_style.font_family = "monospace".to_string();
    let second = add_element(&mut doc, container_id, &mut styles, second_style);
    doc.append_text(second, "a");

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let first_box = find_box(&root, first);
    let second_box = find_box(&root, second);

    assert_eq!(second_box.dimensions.content.x, 50.0);

    let first_baseline = first_line_baseline(first_box).expect("first item baseline");
    let second_baseline = first_line_baseline(second_box).expect("second item baseline");
    assert!(
        (first_baseline - second_baseline).abs() < 0.01,
        "baselines must coincide: first={first_baseline} second={second_baseline}"
    );
    assert!(
        (first_box.dimensions.content.y - second_box.dimensions.content.y).abs() > 0.1,
        "different font sizes should yield different cross offsets when aligned by baseline"
    );
}

#[test]
fn test_inline_flex_participates_in_inline_context() {
    let (mut doc, mut styles, html) = setup();
    let mut container = flex_container_style(Length::Px(200.0), Length::Px(40.0));
    container.display = Display::InlineBlock;
    container.is_inline_flex = true;
    let container_id = add_element(&mut doc, html, &mut styles, container);
    let first = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let second = add_element(
        &mut doc,
        container_id,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let inline_box = find_box_opt(&root, container_id).expect("inline-flex box");
    assert!((inline_box.dimensions.content.width - 200.0).abs() < 0.01);
    let first_box = find_box(&root, first);
    let second_box = find_box(&root, second);
    assert_eq!(
        second_box.dimensions.content.x,
        first_box.dimensions.content.x + 50.0
    );
}

#[test]
fn test_container_without_flex_is_unchanged() {
    let (mut doc, mut styles, html) = setup();
    let mut plain = block_style();
    plain.width = Length::Px(300.0);
    let container = add_element(&mut doc, html, &mut styles, plain);
    let item = add_element(
        &mut doc,
        container,
        &mut styles,
        item_style(Length::Px(50.0), Length::Px(20.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let container_box = find_box(&root, container);
    let item_box = find_box(&root, item);
    assert_eq!(container_box.dimensions.content.width, 300.0);
    assert_eq!(item_box.dimensions.content.x, 0.0);
    assert_eq!(item_box.dimensions.content.y, 0.0);
}

#[test]
fn test_order_reorders_flex_items_stably() {
    let mut first = item_style(Length::Px(50.0), Length::Px(10.0));
    first.order = 5;
    let mut second = item_style(Length::Px(50.0), Length::Px(10.0));
    second.order = 1;
    let mut third = item_style(Length::Px(50.0), Length::Px(10.0));
    third.order = 1;
    let case = layout_case(
        flex_container_style(Length::Px(300.0), Length::Px(20.0)),
        vec![first, second, third],
    );
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
    assert_eq!(case.item(2).dimensions.content.x, 50.0);
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
}

#[test]
fn test_column_flex_auto_height_measures_items_and_vertical_edges() {
    let (mut doc, mut styles, html) = setup();
    let mut container = flex_container_style(Length::Px(300.0), Length::Auto);
    container.flex_direction = FlexDirection::Column;
    let container_id = add_element(&mut doc, html, &mut styles, container);

    let mut first_style = item_style(Length::Auto, Length::Auto);
    first_style.padding.top = Length::Px(4.0);
    first_style.padding.bottom = Length::Px(6.0);
    first_style.margin.bottom = Length::Px(10.0);
    let first = add_element(&mut doc, container_id, &mut styles, first_style);
    let text1 = doc.create_text_node("hello column flex");
    assert!(doc.append_child(first, text1));

    let second_style = item_style(Length::Auto, Length::Auto);
    let second = add_element(&mut doc, container_id, &mut styles, second_style);
    let text2 = doc.create_text_node("second item");
    assert!(doc.append_child(second, text2));

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let container_box = find_box(&root, container_id);
    let first_box = find_box(&root, first);
    let second_box = find_box(&root, second);

    assert!(first_box.dimensions.content.height > 0.0);
    assert!(second_box.dimensions.content.height > 0.0);
    let expected_second_y = first_box.dimensions.content.y
        + first_box.dimensions.content.height
        + first_box.dimensions.padding.bottom
        + first_box.dimensions.margin.bottom;
    assert!((second_box.dimensions.content.y - expected_second_y).abs() < 0.01);
    let expected_container_height = first_box.dimensions.content.height
        + 4.0
        + 6.0
        + 10.0
        + second_box.dimensions.content.height;
    assert!((container_box.dimensions.content.height - expected_container_height).abs() < 0.01);
}

#[test]
fn nested_flex_row_nav_tabs_do_not_overlap() {
    let (mut doc, mut styles, html) = setup();
    let toolbar = add_element(
        &mut doc,
        html,
        &mut styles,
        flex_container_style(Length::Px(800.0), Length::Auto),
    );
    let left_nav = add_element(
        &mut doc,
        toolbar,
        &mut styles,
        item_style(Length::Auto, Length::Auto),
    );
    let mut tab_list_style = flex_container_style(Length::Auto, Length::Auto);
    tab_list_style.flex_grow = 0.0;
    let tab_list = add_element(&mut doc, left_nav, &mut styles, tab_list_style);

    let mut li1_style = item_style(Length::Auto, Length::Auto);
    li1_style.margin.left = Length::Px(8.0);
    li1_style.margin.right = Length::Px(8.0);
    let li1 = add_element(&mut doc, tab_list, &mut styles, li1_style);
    let t1 = doc.create_text_node("Artículo");
    assert!(doc.append_child(li1, t1));

    let mut li2_style = item_style(Length::Auto, Length::Auto);
    li2_style.margin.left = Length::Px(8.0);
    li2_style.margin.right = Length::Px(8.0);
    let li2 = add_element(&mut doc, tab_list, &mut styles, li2_style);
    let t2 = doc.create_text_node("Discusión");
    assert!(doc.append_child(li2, t2));

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let li1_box = find_box(&root, li1);
    let li2_box = find_box(&root, li2);
    assert!(li1_box.dimensions.content.width > 20.0);
    assert!(
        li1_box.dimensions.content.x
            + li1_box.dimensions.content.width
            + li1_box.dimensions.margin.right
            <= li2_box.dimensions.content.x - li2_box.dimensions.margin.left + 0.01,
        "tabs overlapped: li1={:?} li2={:?}",
        li1_box.dimensions,
        li2_box.dimensions
    );
}
