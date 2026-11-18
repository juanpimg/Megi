use super::*;

// ---------------------------------------------------------------------------
// CSS Sizing 3: box-sizing, min/max sizes and over-constrained equations
// ---------------------------------------------------------------------------

#[test]
fn test_box_sizing_border_box_reduces_content_box() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(200.0), Length::Px(100.0));
    style.box_sizing = BoxSizing::BorderBox;
    style.padding = RectEdges::all(Length::Px(10.0));
    style.border_width = RectEdges::all(5.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    // width: 200px includes padding (2*10) and border (2*5).
    assert_close(
        box_.dimensions.content.width,
        170.0,
        "border-box content width",
    );
    assert_close(
        box_.dimensions.border_box().width,
        200.0,
        "border-box width",
    );
    assert_close(
        box_.dimensions.content.height,
        70.0,
        "border-box content height",
    );
    assert_close(
        box_.dimensions.border_box().height,
        100.0,
        "border-box height",
    );

    // content-box keeps the width as the content width (CSS Box Model default).
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(200.0), Length::Px(100.0));
    style.padding = RectEdges::all(Length::Px(10.0));
    style.border_width = RectEdges::all(5.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(
        box_.dimensions.content.width,
        200.0,
        "content-box content width",
    );
    assert_close(
        box_.dimensions.border_box().width,
        230.0,
        "content-box border-box width",
    );
}

#[test]
fn test_min_max_width_clamp_used_width() {
    // CSS 2.2 §10.4: min-width wins over max-width when max < min.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Auto);
    style.min_width = Length::Px(500.0);
    style.max_width = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        500.0,
        "min-width wins",
    );

    // width: 300px clamped by max-width: 100px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(300.0), Length::Auto);
    style.max_width = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        100.0,
        "max-width clamps",
    );

    // width: 100px clamped by min-width: 500px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(100.0), Length::Auto);
    style.min_width = Length::Px(500.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.width,
        500.0,
        "min-width clamps",
    );
}

#[test]
fn test_max_width_with_auto_margins_centers() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Auto);
    style.max_width = Length::Px(200.0);
    style.margin.left = Length::Auto;
    style.margin.right = Length::Auto;
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(box_.dimensions.content.width, 200.0, "clamped width");
    assert_close(box_.dimensions.margin.left, 300.0, "auto left margin");
    assert_close(box_.dimensions.margin.right, 300.0, "auto right margin");
    assert_close(box_.dimensions.content.x, 300.0, "centered x");
}

#[test]
fn test_min_max_height_clamp_used_height() {
    // height: 10px raised by min-height: 50px.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Px(10.0));
    style.min_height = Length::Px(50.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.height,
        50.0,
        "min-height",
    );

    // Auto height of a 100px-tall child clamped by max-height: 30px.
    let (mut doc, mut styles, html) = setup_html_only();
    let wrapper = make_div(&mut doc, html);
    let child = make_div(&mut doc, wrapper);
    let mut wrapper_style = make_block_style(Length::Auto, Length::Auto);
    wrapper_style.max_height = Length::Px(30.0);
    styles.insert(wrapper, wrapper_style);
    styles.insert(child, make_block_style(Length::Auto, Length::Px(100.0)));

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    assert_close(
        tree.children[0].dimensions.content.height,
        30.0,
        "auto height clamped by max-height",
    );

    // border-box min-height constrains the border box.
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Auto, Length::Px(10.0));
    style.box_sizing = BoxSizing::BorderBox;
    style.padding = RectEdges::all(Length::Px(5.0));
    style.min_height = Length::Px(40.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    assert_close(
        box_.dimensions.content.height,
        30.0,
        "border-box min-height content",
    );
    assert_close(
        box_.dimensions.border_box().height,
        40.0,
        "border-box min-height border box",
    );
}

#[test]
fn test_overconstrained_ltr_ignores_margin_right() {
    let (mut doc, mut styles, html) = setup_html_only();
    let div = make_div(&mut doc, html);
    let mut style = make_block_style(Length::Px(400.0), Length::Auto);
    style.margin.left = Length::Px(20.0);
    style.margin.right = Length::Px(100.0);
    styles.insert(div, style);

    let tree = layout_doc(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
    let box_ = &tree.children[0];
    // 20 + 400 + margin-right = 800 => margin-right is solved as 380 (LTR).
    assert_close(box_.dimensions.margin.left, 20.0, "margin-left");
    assert_close(box_.dimensions.margin.right, 380.0, "margin-right solved");
    assert_close(box_.dimensions.content.x, 20.0, "content x");
    assert_close(box_.dimensions.content.width, 400.0, "content width");
}
