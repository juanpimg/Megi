use super::common::*;

#[test]
fn test_auto_flow_column_fills_down_columns() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    container.grid_template_rows = template(vec![px(20.0), px(20.0)]);
    container.grid_auto_flow = megi_css::GridAutoFlow::Column;
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // Column flow: items go (r0,c0), (r1,c0), (r0,c1).
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert!((case.item(1).dimensions.content.y - 20.0).abs() < 0.01);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
    assert_eq!(case.item(2).dimensions.content.y, 0.0);
}

#[test]
fn test_auto_flow_row_dense_backfills_holes() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    container.grid_auto_flow = megi_css::GridAutoFlow::RowDense;
    let mut explicit = item_style(Length::Auto, Length::Px(10.0));
    explicit.grid_column_start = megi_css::GridLine::Line(3);
    let case = layout_case(
        container,
        vec![
            explicit,
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // Explicit item in column 3; dense packing backfills columns 1 and 2 in row 0.
    assert_eq!(case.item(0).dimensions.content.x, 200.0);
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
    assert!((case.item(1).dimensions.content.y).abs() < 0.01);
}

#[test]
fn test_template_areas_and_grid_area_name() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    container.grid_template_rows = template(vec![px(40.0), px(40.0)]);
    container.grid_template_areas = Some(megi_css::GridTemplateAreas {
        rows: 2,
        columns: 2,
        cells: vec![
            Some("header".to_owned()),
            Some("header".to_owned()),
            Some("main".to_owned()),
            Some("side".to_owned()),
        ],
    });
    let mut header = item_style(Length::Auto, Length::Px(10.0));
    header.grid_area = Some("header".to_owned());
    let mut main = item_style(Length::Auto, Length::Px(10.0));
    main.grid_area = Some("main".to_owned());
    let mut side = item_style(Length::Auto, Length::Px(10.0));
    side.grid_area = Some("side".to_owned());
    let case = layout_case(container, vec![header, main, side]);
    // `header` spans both columns of row 0.
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
    assert_eq!(case.item(0).dimensions.content.y, 0.0);
    assert_eq!(case.item(1).dimensions.content.y, 40.0);
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
    assert_eq!(case.item(2).dimensions.content.x, 100.0);
    assert_eq!(case.item(2).dimensions.content.y, 40.0);
}

#[test]
fn test_grid_area_fills_axes_without_an_explicit_longhand() {
    // #1709: grid-area is a shorthand for the four placement longhands. An
    // explicit column-start must override only that axis; the row placement
    // from the named area must still apply.
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    container.grid_template_rows = template(vec![px(40.0), px(40.0)]);
    container.grid_template_areas = Some(megi_css::GridTemplateAreas {
        rows: 2,
        columns: 2,
        cells: vec![
            Some("header".to_owned()),
            Some("header".to_owned()),
            Some("main".to_owned()),
            Some("side".to_owned()),
        ],
    });
    let mut item = item_style(Length::Auto, Length::Px(10.0));
    item.grid_area = Some("main".to_owned());
    item.grid_column_start = megi_css::GridLine::Line(2);
    let case = layout_case(container, vec![item]);
    assert!(
        (case.item(0).dimensions.content.y - 40.0).abs() < 0.01,
        "the row from grid-area must survive an explicit column-start"
    );
    assert!(
        (case.item(0).dimensions.content.x - 100.0).abs() < 0.01,
        "the explicit column-start must win on its own axis"
    );
}

#[test]
fn test_named_template_lines_place_by_name() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    let mut columns = template(vec![px(100.0), px(100.0), px(100.0)]);
    columns.line_names = vec![
        vec!["left".to_owned()],
        vec!["mid".to_owned()],
        vec!["center".to_owned()],
        vec!["right".to_owned()],
    ];
    container.grid_template_columns = columns;
    let mut across = item_style(Length::Auto, Length::Px(10.0));
    across.grid_column_start = megi_css::GridLine::Name("mid".to_owned());
    across.grid_column_end = megi_css::GridLine::Name("right".to_owned());
    let mut first = item_style(Length::Auto, Length::Px(10.0));
    first.grid_column_start = megi_css::GridLine::Name("left".to_owned());
    let case = layout_case(container, vec![across, first]);
    assert_eq!(case.item(0).dimensions.content.x, 100.0);
    assert_eq!(case.item(0).dimensions.content.width, 200.0);
    assert_eq!(case.item(1).dimensions.content.x, 0.0);
}

#[test]
fn test_justify_content_center_and_space_between() {
    let mut centered = grid_container_style(Length::Px(300.0), Length::Auto);
    centered.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    centered.grid_justify_content = megi_css::GridContentDistribution::Center;
    let case = layout_case(
        centered,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert!((case.item(0).dimensions.content.x - 50.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.x - 150.0).abs() < 0.01);

    let mut between = grid_container_style(Length::Px(300.0), Length::Auto);
    between.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    between.grid_justify_content = megi_css::GridContentDistribution::SpaceBetween;
    let case = layout_case(
        between,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    assert!((case.item(0).dimensions.content.x).abs() < 0.01);
    assert!((case.item(1).dimensions.content.x - 200.0).abs() < 0.01);
}

#[test]
fn test_align_content_center_distributes_rows() {
    let mut container = grid_container_style(Length::Px(100.0), Length::Px(200.0));
    container.grid_template_columns = template(vec![fr(1.0)]);
    container.grid_template_rows = template(vec![px(40.0), px(40.0)]);
    container.grid_align_content = megi_css::GridContentDistribution::Center;
    let case = layout_case(
        container,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // Used rows = 80px; leftover 120px → leading 60px.
    assert!((case.item(0).dimensions.content.y - 60.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.y - 100.0).abs() < 0.01);
}

#[test]
fn test_auto_fit_collapses_empty_tracks() {
    let mut container = grid_container_style(Length::Px(300.0), Length::Auto);
    container.column_gap = Length::Px(10.0);
    container.grid_template_columns = GridTemplate {
        tracks: Vec::new(),
        auto_repeat: Some(megi_css::AutoRepeat {
            kind: megi_css::AutoRepeatKind::Fit,
            tracks: vec![TrackSize::MinMax {
                min: megi_css::TrackMin::Fixed(Length::Px(80.0)),
                max: megi_css::TrackMax::Fraction(1.0),
            }],
        }),
        line_names: Vec::new(),
    };
    let case = layout_case(container, vec![item_style(Length::Auto, Length::Px(10.0))]);
    // 3 tracks fit, but the two empty auto-fit tracks collapse: the single item
    // stretches over the whole container with no gutters.
    assert!((case.item(0).dimensions.content.width - 300.0).abs() < 0.01);
    assert!((case.item(0).dimensions.content.x).abs() < 0.01);
}

#[test]
fn test_area_derived_named_lines() {
    let mut container = grid_container_style(Length::Px(200.0), Length::Auto);
    container.grid_template_columns = template(vec![px(100.0), px(100.0)]);
    container.grid_template_rows = template(vec![px(40.0)]);
    container.grid_template_areas = Some(megi_css::GridTemplateAreas {
        rows: 1,
        columns: 2,
        cells: vec![Some("banner".to_owned()), Some("banner".to_owned())],
    });
    let mut item = item_style(Length::Auto, Length::Px(10.0));
    item.grid_column_start = megi_css::GridLine::Name("banner-start".to_owned());
    item.grid_column_end = megi_css::GridLine::Name("banner-end".to_owned());
    let case = layout_case(container, vec![item]);
    assert_eq!(case.item(0).dimensions.content.x, 0.0);
    assert!((case.item(0).dimensions.content.width - 200.0).abs() < 0.01);
}

#[test]
fn test_content_distribution_end_and_space_evenly() {
    let mut end = grid_container_style(Length::Px(300.0), Length::Auto);
    end.grid_template_columns = template(vec![px(100.0)]);
    end.grid_justify_content = megi_css::GridContentDistribution::End;
    let case = layout_case(end, vec![item_style(Length::Auto, Length::Px(10.0))]);
    assert!((case.item(0).dimensions.content.x - 200.0).abs() < 0.01);

    let mut evenly = grid_container_style(Length::Px(300.0), Length::Auto);
    evenly.grid_template_columns = template(vec![px(60.0), px(60.0)]);
    evenly.grid_justify_content = megi_css::GridContentDistribution::SpaceEvenly;
    let case = layout_case(
        evenly,
        vec![
            item_style(Length::Auto, Length::Px(10.0)),
            item_style(Length::Auto, Length::Px(10.0)),
        ],
    );
    // leftover 180; share = 180 / 3 = 60 → x = 60 and 60+60+60 = 180.
    assert!((case.item(0).dimensions.content.x - 60.0).abs() < 0.01);
    assert!((case.item(1).dimensions.content.x - 180.0).abs() < 0.01);
}
