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
