use super::*;
use crate::Color;

fn sample_list() -> DisplayList {
    let runs = Arc::new(vec![TextMaskRun {
        text: "Mask".to_owned(),
        rect: Rect::new(2.0, 3.0, 40.0, 16.0),
        font_size: 10.0,
        font_family: "sans".to_owned(),
        font_weight: 400,
        font_italic: false,
        advance: 5.0,
        baseline: 8.0,
    }]);
    DisplayList {
        items: vec![
            DisplayItem::Text {
                text: "Hi".to_owned(),
                rect: Rect::new(10.0, 20.0, 100.0, 30.0),
                color: Color::BLACK,
                font_size: 16.0,
                font_family: "sans".to_owned(),
                font_weight: 400,
                font_italic: false,
                advance: 12.0,
                baseline: 14.0,
            },
            DisplayItem::RoundedRect {
                rect: Rect::new(1.0, 2.0, 20.0, 20.0),
                radii: BorderRadii {
                    top_left: Radius {
                        x: Length::Px(4.0),
                        y: Length::Percent(25.0),
                    },
                    top_right: Radius::ZERO,
                    bottom_right: Radius::ZERO,
                    bottom_left: Radius::ZERO,
                },
                color: Color::RED,
            },
            DisplayItem::BoxShadow {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radii: BorderRadii::default(),
                shadow: BoxShadow {
                    offset_x: 3.0,
                    offset_y: 4.0,
                    blur: 8.0,
                    spread: 2.0,
                    color: Color::BLACK,
                    inset: false,
                },
            },
            DisplayItem::OpacityGroup {
                rect: Rect::new(0.0, 0.0, 200.0, 100.0),
                opacity: 0.5,
                items: vec![DisplayItem::SolidColor {
                    rect: Rect::new(5.0, 6.0, 7.0, 8.0),
                    color: Color::RED,
                }],
            },
            DisplayItem::PushTextMaskGroup {
                rect: Rect::new(0.0, 0.0, 40.0, 20.0),
                runs,
            },
            DisplayItem::PopTextMaskGroup,
        ],
    }
}

#[test]
fn dpr_raster_scale_scales_rects_text_radii_shadows_and_nested_groups() {
    let scaled = RasterScale::new(2.0).apply(&sample_list());
    assert_eq!(scaled.items.len(), 6);

    let DisplayItem::Text {
        rect,
        font_size,
        advance,
        baseline,
        text,
        ..
    } = &scaled.items[0]
    else {
        panic!("text item");
    };
    assert_eq!(*rect, Rect::new(20.0, 40.0, 200.0, 60.0));
    assert_eq!(*font_size, 32.0);
    assert_eq!(*advance, 24.0);
    assert_eq!(*baseline, 28.0);
    assert_eq!(text, "Hi");

    let DisplayItem::RoundedRect { rect, radii, .. } = &scaled.items[1] else {
        panic!("rounded rect item");
    };
    assert_eq!(*rect, Rect::new(2.0, 4.0, 40.0, 40.0));
    assert_eq!(radii.top_left.x, Length::Px(8.0));
    assert_eq!(radii.top_left.y, Length::Percent(25.0));

    let DisplayItem::BoxShadow { rect, shadow, .. } = &scaled.items[2] else {
        panic!("box shadow item");
    };
    assert_eq!(*rect, Rect::new(0.0, 0.0, 20.0, 20.0));
    assert_eq!(shadow.offset_x, 6.0);
    assert_eq!(shadow.offset_y, 8.0);
    assert_eq!(shadow.blur, 16.0);
    assert_eq!(shadow.spread, 4.0);

    let DisplayItem::OpacityGroup {
        rect,
        items,
        opacity,
    } = &scaled.items[3]
    else {
        panic!("opacity group item");
    };
    assert_eq!(*rect, Rect::new(0.0, 0.0, 400.0, 200.0));
    assert_eq!(*opacity, 0.5);
    assert_eq!(
        items,
        &vec![DisplayItem::SolidColor {
            rect: Rect::new(10.0, 12.0, 14.0, 16.0),
            color: Color::RED,
        }]
    );

    let DisplayItem::PushTextMaskGroup { rect, runs } = &scaled.items[4] else {
        panic!("text mask group item");
    };
    assert_eq!(*rect, Rect::new(0.0, 0.0, 80.0, 40.0));
    assert_eq!(runs[0].rect, Rect::new(4.0, 6.0, 80.0, 32.0));
    assert_eq!(runs[0].font_size, 20.0);
    assert_eq!(runs[0].advance, 10.0);
    assert_eq!(runs[0].baseline, 16.0);
    assert_eq!(scaled.items[5], DisplayItem::PopTextMaskGroup);
}

#[test]
fn dpr_raster_scale_one_is_byte_identical() {
    let list = sample_list();
    assert_eq!(RasterScale::ONE.apply(&list), list);
    assert_eq!(RasterScale::new(f32::NAN), RasterScale::ONE);
    assert_eq!(RasterScale::new(0.0), RasterScale::ONE);
}

#[test]
fn dpr_raster_scale_sanitizes_out_of_range_factors() {
    assert_eq!(RasterScale::new(100.0).factor(), MAX_RASTER_SCALE);
    assert_eq!(RasterScale::new(0.01).factor(), MIN_RASTER_SCALE);
}

#[test]
fn dpr_raster_scale_scales_filters_and_gradient_lengths() {
    let list = DisplayList {
        items: vec![
            DisplayItem::BackdropFilter {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                filters: vec![
                    FilterFunction::Blur(4.0),
                    FilterFunction::Brightness(0.5),
                    FilterFunction::DropShadow {
                        offset_x: 1.0,
                        offset_y: 2.0,
                        blur: 3.0,
                        color: Color::BLACK,
                    },
                ],
            },
            DisplayItem::Gradient {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                gradient: Arc::new(crate::Gradient {
                    kind: crate::GradientKind::Radial(crate::RadialGradient {
                        shape: None,
                        extent: Some(RadialExtent::Explicit(5.0, 6.0)),
                        position: megi_css::BackgroundPosition {
                            x: Length::Px(1.0),
                            y: Length::Percent(50.0),
                        },
                    }),
                    repeating: false,
                    stops: vec![crate::GradientStop {
                        color: Color::RED,
                        position: Some(Length::Px(2.0)),
                    }],
                    hints: vec![Some(Length::Px(3.0))],
                    interpolation: crate::InterpolationSpace::default(),
                }),
            },
        ],
    };
    let scaled = RasterScale::new(2.0).apply(&list);
    let DisplayItem::BackdropFilter { filters, .. } = &scaled.items[0] else {
        panic!("filter item");
    };
    assert_eq!(filters[0], FilterFunction::Blur(8.0));
    assert_eq!(filters[1], FilterFunction::Brightness(0.5));
    assert_eq!(
        filters[2],
        FilterFunction::DropShadow {
            offset_x: 2.0,
            offset_y: 4.0,
            blur: 6.0,
            color: Color::BLACK,
        }
    );
    let DisplayItem::Gradient { gradient, .. } = &scaled.items[1] else {
        panic!("gradient item");
    };
    assert_eq!(gradient.stops[0].position, Some(Length::Px(4.0)));
    assert_eq!(gradient.hints[0], Some(Length::Px(6.0)));
    let crate::GradientKind::Radial(radial) = &gradient.kind else {
        panic!("radial gradient");
    };
    assert_eq!(radial.extent, Some(RadialExtent::Explicit(10.0, 12.0)));
    assert_eq!(radial.position.x, Length::Px(2.0));
    assert_eq!(radial.position.y, Length::Percent(50.0));
}

#[test]
fn dpr_raster_scale_scales_mask_group_rect() {
    // Upstream mask-image groups carry the same CSS geometry contract.
    let mask = Arc::new(crate::RgbaImage::new(1, 1, vec![255, 255, 255, 255]).expect("mask"));
    let list = DisplayList {
        items: vec![
            DisplayItem::PushMaskGroup {
                rect: Rect::new(1.0, 2.0, 10.0, 20.0),
                mask: Arc::clone(&mask),
            },
            DisplayItem::PopMaskGroup,
        ],
    };
    let scaled = RasterScale::new(1.5).apply(&list);
    let DisplayItem::PushMaskGroup { rect, mask: copied } = &scaled.items[0] else {
        panic!("mask group");
    };
    assert_eq!(*rect, Rect::new(1.5, 3.0, 15.0, 30.0));
    assert!(
        Arc::ptr_eq(copied, &mask),
        "mask bitmap is shared, not copied"
    );
    assert_eq!(scaled.items[1], DisplayItem::PopMaskGroup);
}
