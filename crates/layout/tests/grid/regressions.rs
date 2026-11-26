use super::common::*;

#[test]
fn test_auto_item_span_wider_than_the_implicit_columns_terminates() {
    // One implicit column + `span 2`: the placement loop used to advance the row forever
    // because the column limit never grew; it must widen the implicit grid instead.
    let mut item = item_style(Length::Px(50.0), Length::Px(10.0));
    item.grid_column_start = GridLine::Span(2);
    let case = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![item],
    );
    let laid_out = case.item(0);
    assert!(laid_out.dimensions.content.width.is_finite());
}

#[test]
fn test_auto_item_with_a_huge_span_is_clamped() {
    // `span 4294967295` used to ask `ensure_occupancy` for a 4e9-wide row.
    let mut item = item_style(Length::Px(50.0), Length::Px(10.0));
    item.grid_column_start = GridLine::Span(u32::MAX);
    let case = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![item],
    );
    let laid_out = case.item(0);
    assert!(laid_out.dimensions.content.width.is_finite());
    assert!(laid_out.dimensions.content.width <= 200.001);
}

#[test]
fn test_positive_grid_line_beyond_explicit_creates_implicit_tracks() {
    // #1706: a positive line number past the explicit grid creates implicit
    // tracks; it must not be clamped to the end of the explicit grid.
    let mut container = grid_container_style(Length::Px(600.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    let mut item = item_style(Length::Px(50.0), Length::Px(10.0));
    item.grid_column_start = GridLine::Line(5);
    item.grid_column_end = GridLine::Line(6);
    let case = layout_case(container, vec![item]);
    let x = case.item(0).dimensions.content.x;
    assert!(
        x > 300.0,
        "line 5 must place the item past the two explicit 100px columns, got x={x}"
    );
}

#[test]
fn test_explicit_line_and_end_beyond_the_track_cap_terminate() {
    // `grid-column-start: 2147483647` / `grid-column-end: 2147483647` used to request ~2e9
    // tracks; both are clamped with the span so start + span <= MAX_EXPANDED_TRACKS.
    for line in [GridLine::Line(i32::MAX), GridLine::Line(1_000_000)] {
        let mut item = item_style(Length::Px(20.0), Length::Px(10.0));
        item.grid_column_start = line;
        item.grid_column_end = GridLine::Line(i32::MAX);
        let case = layout_case(
            grid_container_style(Length::Px(200.0), Length::Auto),
            vec![item],
        );
        let laid_out = case.item(0);
        assert!(laid_out.dimensions.content.width.is_finite());
    }
}

#[test]
fn test_grid_item_respects_min_max_width_and_min_wins_over_max() {
    // Item with auto width clamped by max-width inside a 200px track.
    let mut max_clamped = item_style(Length::Auto, Length::Px(20.0));
    max_clamped.max_width = Length::Px(80.0);
    let case_max = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![max_clamped],
    );
    assert!((case_max.item(0).dimensions.content.width - 80.0).abs() < 0.1);

    // When min-width > max-width, CSS 2.2 §10.4 specifies min-width wins.
    let mut min_over_max = item_style(Length::Px(50.0), Length::Px(20.0));
    min_over_max.min_width = Length::Px(120.0);
    min_over_max.max_width = Length::Px(80.0);
    let case_min_wins = layout_case(
        grid_container_style(Length::Px(200.0), Length::Auto),
        vec![min_over_max],
    );
    assert!((case_min_wins.item(0).dimensions.content.width - 120.0).abs() < 0.1);
}

#[test]
fn test_minmax_fixed_min_fr_row_does_not_double_count_minimum() {
    // WHATWG CSS Grid §12.7.1 (#1839) — with `grid-template-rows: minmax(40px, 1fr)`
    // in a 100px-tall container, the 40px minimum is the *floor inside* the free
    // space, not an amount added on top of it. The row must be 100px, not 140px.
    let mut container = grid_container_style(Length::Px(200.0), Length::Px(100.0));
    container.grid_template_rows = template(vec![TrackSize::MinMax {
        min: megi_css::TrackMin::Fixed(Length::Px(40.0)),
        max: megi_css::TrackMax::Fraction(1.0),
    }]);
    // An auto-height item stretches to fill its grid area, so its box height reveals
    // the computed row height even though the container height is fixed at 100px.
    let case = layout_case(container, vec![item_style(Length::Auto, Length::Auto)]);
    let row_height = case.item(0).dimensions.content.height;
    assert!(
        (row_height - 100.0).abs() < 0.5,
        "minmax(40px,1fr) row in 100px container must be 100px, got {row_height}"
    );
}

fn style_from_inline_css(css: &str) -> ComputedStyle {
    let declarations = megi_css::parse_inline_declarations(css);
    let mut cascaded = std::collections::HashMap::new();
    for declaration in &declarations {
        cascaded.insert(declaration.name.clone(), declaration);
    }
    ComputedStyle::compute(&cascaded, None, None)
}

#[test]
fn test_grid_template_shorthand_areas_keep_article_beside_tall_sidebar() {
    // CSS Grid Layout §7.4: `grid-template` is a shorthand for the row/column
    // tracks and the named areas. Wikipedia Vector 2022's
    // `.mw-page-container-inner` uses exactly this shape; when the shorthand is
    // dropped the container has a single implicit column and the article is
    // stacked *below* the ~440px sidebar instead of beside it.
    let container = style_from_inline_css(
        "display: grid; width: 1000px; column-gap: 24px; \
         grid-template: min-content 1fr min-content / 100px minmax(0,1fr); \
         grid-template-areas: 'siteNotice siteNotice' 'columnStart pageContent' 'footer footer';",
    );
    let notice = style_from_inline_css("display: block; grid-area: siteNotice;");
    let sidebar = style_from_inline_css("display: block; grid-area: columnStart; height: 440px;");
    let article = style_from_inline_css("display: block; grid-area: pageContent; height: 30px;");
    let footer = style_from_inline_css("display: block; grid-area: footer;");

    let (mut doc, mut styles, html) = setup();
    let container_id = add_element(&mut doc, html, &mut styles, container);
    add_element(&mut doc, container_id, &mut styles, notice);
    let sidebar_id = add_element(&mut doc, container_id, &mut styles, sidebar);
    let article_id = add_element(&mut doc, container_id, &mut styles, article);
    add_element(&mut doc, container_id, &mut styles, footer);
    let root =
        layout_document(&doc, &styles, Rect::new(0.0, 0.0, 1000.0, 700.0)).expect("layout result");

    let sidebar = find_box(&root, sidebar_id);
    let article = find_box(&root, article_id);
    assert!(
        (article.dimensions.content.y - sidebar.dimensions.content.y).abs() < 1.0,
        "the h1/article must share the row with the tall TOC sidebar (CSS Grid §7.4/§7.3): \
         article y={}, sidebar y={}",
        article.dimensions.content.y,
        sidebar.dimensions.content.y,
    );
    assert!(
        article.dimensions.content.x >= 100.0,
        "article must start after the 100px sidebar column, got x={}",
        article.dimensions.content.x,
    );
}

#[test]
fn test_minmax_percentage_min_row_with_indefinite_height_uses_zero_floor() {
    // CSS Grid Layout §7.2.1 / §11.5 (#2185): percentage track minimums on rows when
    // the grid container has indefinite height resolve against nothing (behaving as
    // `auto`/0 floor for minmax), NOT against container width.
    let mut container = grid_container_style(Length::Px(1000.0), Length::Auto);
    container.grid_template_rows = template(vec![TrackSize::MinMax {
        min: megi_css::TrackMin::Fixed(Length::Percent(10.0)),
        max: megi_css::TrackMax::Fixed(Length::Px(50.0)),
    }]);
    let case = layout_case(container, vec![item_style(Length::Auto, Length::Auto)]);
    let row_height = case.item(0).dimensions.content.height;
    assert!(
        (row_height - 50.0).abs() < 0.5,
        "minmax(10%, 50px) row with auto container height must be 50px, not 10% of 1000px width (got {row_height})"
    );

    let mut auto_max = grid_container_style(Length::Px(1000.0), Length::Auto);
    auto_max.grid_template_rows = template(vec![TrackSize::MinMax {
        min: megi_css::TrackMin::Fixed(Length::Percent(10.0)),
        max: megi_css::TrackMax::Auto,
    }]);
    let mut min_item = item_style(Length::Auto, Length::Auto);
    min_item.min_height = Length::Px(20.0);
    let case_auto = layout_case(auto_max, vec![min_item]);
    let auto_row_height = case_auto.item(0).dimensions.content.height;
    assert!(
        (auto_row_height - 20.0).abs() < 0.5,
        "minmax(10%, auto) row with 20px item and auto container height must be 20px (got {auto_row_height})"
    );
}
