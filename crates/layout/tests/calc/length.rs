use super::*;

#[test]
fn width_calc_percent_minus_px() {
    let mut style = block(Length::Auto, Length::Px(20.0));
    style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 100.0,
        px: -40.0,
    });
    let dims = simple(style, 800.0);
    assert!((dims.width - 760.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn width_calc_half_plus_px_inside_parent() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Auto, Length::Px(10.0));
    child_style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.width - 210.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn height_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(200.0), Length::Px(200.0)),
    );
    let mut child_style = block(Length::Px(50.0), Length::Auto);
    child_style.calc_lengths.height = Some(megi_css::calc::CalcValue {
        percent: 100.0,
        px: -20.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.height - 180.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn margin_left_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Px(100.0), Length::Px(10.0));
    child_style.calc_lengths.margin_left = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: -20.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.x - 180.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn padding_top_calc_percent_plus_px() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(200.0), Length::Auto),
    );
    let mut child_style = block(Length::Px(50.0), Length::Px(10.0));
    child_style.calc_lengths.padding_top = Some(megi_css::calc::CalcValue {
        percent: 10.0,
        px: 5.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.y - 25.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn absolute_top_calc_percent_minus_px() {
    let (mut doc, mut styles, html) = setup();
    let mut parent_style = block(Length::Px(200.0), Length::Px(200.0));
    parent_style.position = Position::Relative;
    let parent = add(&mut doc, html, &mut styles, parent_style);
    let mut child_style = block(Length::Px(50.0), Length::Px(50.0));
    child_style.position = Position::Absolute;
    child_style.calc_lengths.top = Some(megi_css::calc::CalcValue {
        percent: 100.0,
        px: -30.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.y - 170.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn nested_calc_em_and_multiplication() {
    let mut style = block(Length::Auto, Length::Px(10.0));
    style.calc_lengths.width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 20.0,
    });
    let dims = simple(style, 300.0);
    assert!((dims.width - 170.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn max_width_calc_clamps_stretch_width() {
    let (mut doc, mut styles, html) = setup();
    let parent = add(
        &mut doc,
        html,
        &mut styles,
        block(Length::Px(400.0), Length::Auto),
    );
    let mut child_style = block(Length::Auto, Length::Px(10.0));
    child_style.calc_lengths.max_width = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let child = add(&mut doc, parent, &mut styles, child_style);
    let root = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout");
    let dims = dimensions(find_box(&root, child));
    assert!((dims.width - 210.0).abs() < 0.01, "{dims:?}");
}

#[test]
fn indefinite_height_uses_absolute_part_only() {
    let mut style = block(Length::Px(50.0), Length::Auto);
    style.calc_lengths.height = Some(megi_css::calc::CalcValue {
        percent: 50.0,
        px: 10.0,
    });
    let dims = simple(style, 300.0);
    assert!((dims.height - 10.0).abs() < 0.01, "{dims:?}");
}
