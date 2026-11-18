use super::*;

#[test]
fn flex_gap_calc_em_plus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(300.0), Length::Px(20.0));
    container_style.display = Display::Flex;
    let container = add(&mut doc, html, &mut styles, container_style);
    let mut first_style = block(Length::Px(50.0), Length::Px(10.0));
    first_style.display = Display::Block;
    let first = add(&mut doc, container, &mut styles, first_style);
    let mut second_style = block(Length::Px(50.0), Length::Px(10.0));
    second_style.display = Display::Block;
    let second = add(&mut doc, container, &mut styles, second_style);
    // Store the mixed gap directly on the container style before layout.
    let mut styles2 = styles;
    let mut s = styles2.get(&container).cloned().expect("style");
    s.calc_lengths.column_gap = Some(megi_css::calc::CalcValue {
        percent: 0.0,
        px: 21.0,
    });
    styles2.insert(container, s);
    let root = layout_document(&doc, &styles2, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let first_x = dimensions(find_box(&root, first)).x;
    let second_x = dimensions(find_box(&root, second)).x;
    assert!((first_x).abs() < 0.01, "{first_x:?}");
    assert!((second_x - 71.0).abs() < 0.01, "{second_x}");
}

#[test]
fn absolute_left_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut parent_style = block(Length::Px(200.0), Length::Px(200.0));
    parent_style.position = Position::Relative;
    let parent = add(&mut doc, html, &mut styles, parent_style);
    let mut child_style = block(Length::Px(50.0), Length::Px(50.0));
    child_style.position = Position::Absolute;
    child_style.calc_lengths.left = Some(megi_css::calc::CalcValue {
        percent: 100.0,
        px: -40.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.x - 160.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn grid_column_gap_calc_mixed() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(200.0), Length::Auto);
    container_style.display = Display::Grid;
    container_style.grid_template_columns = megi_css::GridTemplate {
        tracks: vec![
            megi_css::TrackSize::Fixed(Length::Px(100.0)),
            megi_css::TrackSize::Fixed(Length::Px(100.0)),
        ],
        auto_repeat: None,
        line_names: Vec::new(),
    };
    container_style.calc_lengths.column_gap = Some(megi_css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    let container = add(&mut doc, html, &mut styles, container_style);
    let first = add(
        &mut doc,
        container,
        &mut styles,
        block(Length::Auto, Length::Px(10.0)),
    );
    let second = add(
        &mut doc,
        container,
        &mut styles,
        block(Length::Auto, Length::Px(10.0)),
    );
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    // Two explicit 100px columns with a 10% + 5px = 25px gap between them.
    let first_x = dimensions(find_box(&root, first)).x;
    let second_x = dimensions(find_box(&root, second)).x;
    assert!((first_x).abs() < 0.01, "{first_x:?}");
    assert!((second_x - 125.0).abs() < 0.01, "{second_x:?}");
}

#[test]
fn flex_item_width_calc_mixed() {
    let (mut doc, mut styles, html) = setup();
    let mut container_style = block(Length::Px(300.0), Length::Px(20.0));
    container_style.display = Display::Flex;
    let container = add(&mut doc, html, &mut styles, container_style);
    let mut item_style = block(Length::Auto, Length::Px(10.0));
    item_style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: -10.0,
    });
    let item = add(&mut doc, container, &mut styles, item_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, item));
    assert!((dims.width - 140.0).abs() < 0.01, "{dims:?}");
}

/// Hostile lengths must not reach box geometry as `inf`/`NaN` (#693).
#[test]
fn infinite_and_nan_lengths_are_clamped_to_finite_geometry() {
    for (width, height, padding) in [
        (Length::Px(f32::INFINITY), Length::Px(f32::INFINITY), 0.0),
        (
            Length::Px(f32::NEG_INFINITY),
            Length::Px(f32::NAN),
            f32::NAN,
        ),
        (Length::Px(1.0e30), Length::Px(-1.0e30), 1.0e30),
        (Length::Percent(1.0e30), Length::Percent(f32::INFINITY), 0.0),
    ] {
        let (mut doc, mut styles, html) = setup();
        let mut hostile = block(width, height);
        hostile.padding.top = Length::Px(padding);
        hostile.padding.left = Length::Px(padding);
        let div = add(&mut doc, html, &mut styles, hostile);
        let tree =
            layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
        let div_box = find_box(&tree, div);
        for rect in [
            div_box.dimensions.content,
            div_box.dimensions.padding_box(),
            div_box.dimensions.border_box(),
            div_box.dimensions.margin_box(),
        ] {
            assert!(
                rect.x.is_finite()
                    && rect.y.is_finite()
                    && rect.width.is_finite()
                    && rect.height.is_finite(),
                "hostile length reached geometry: {rect:?}"
            );
            // Each edge is clamped to 1e8, so a box can reach roughly 3x that once the
            // padding/border/margin layers are added on both sides — still finite and far
            // below the `f32 -> i32` overflow that used to fire in paint.
            assert!(
                rect.width.abs() <= 4.0e8 && rect.height.abs() <= 4.0e8,
                "hostile length was not bounded: {rect:?}"
            );
        }
    }
}

#[test]
fn float_shrink_to_fit_clamps_min_and_max_width() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );

    // Float with calc(50% + 50px) = 250px clamped by max-width: 180px.
    let mut float_max_style = block(Length::Auto, Length::Px(20.0));
    float_max_style.float = megi_css::Float::Left;
    float_max_style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 50.0,
    });
    float_max_style.max_width = Length::Px(180.0);
    let float_max = add(&mut doc, container, &mut styles, float_max_style);

    // Float with width: 40px clamped up by min-width: 120px.
    let mut float_min_style = block(Length::Px(40.0), Length::Px(20.0));
    float_min_style.float = megi_css::Float::Left;
    float_min_style.min_width = Length::Px(120.0);
    let float_min = add(&mut doc, container, &mut styles, float_min_style);

    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let w_max = dimensions(find_box(&root, float_max)).width;
    let w_min = dimensions(find_box(&root, float_min)).width;
    assert!((w_max - 180.0).abs() < 0.01, "expected 180.0, got {w_max}");
    assert!((w_min - 120.0).abs() < 0.01, "expected 120.0, got {w_min}");
}

#[test]
fn replaced_element_respects_calc_and_min_max_constraints() {
    let (mut doc, mut styles, html) = setup();
    let container = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let img = doc.create_element(Namespace::Html, "img", vec![]);
    doc.append_child(container, img);
    let mut img_style = ComputedStyle::initial();
    img_style.display = Display::Block;
    // calc(50% + 100px) = 300px, clamped by max-width: 160px -> height should scale to 80px.
    img_style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 100.0,
    });
    img_style.max_width = Length::Px(160.0);
    styles.insert(img, img_style);
    let mut sizes = HashMap::new();
    sizes.insert(img, (200.0, 100.0));

    let root = megi_layout::layout_document_with_replaced_sizes(
        &doc,
        &styles,
        Rect::new(0.0, 0.0, 800.0, 600.0),
        &sizes,
    )
    .expect("layout");
    fn find_replaced(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
        if matches!(root.box_type, BoxType::Replaced(node) if node == id) {
            return Some(root);
        }
        for child in &root.children {
            if let Some(found) = find_replaced(child, id) {
                return Some(found);
            }
        }
        None
    }
    let img_box = find_replaced(&root, img).expect("replaced box");
    assert!(
        (img_box.dimensions.content.width - 160.0).abs() < 0.01,
        "{:?}",
        img_box.dimensions.content
    );
    assert!(
        (img_box.dimensions.content.height - 80.0).abs() < 0.01,
        "{:?}",
        img_box.dimensions.content
    );
}
