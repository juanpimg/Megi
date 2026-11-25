//! Bounds regressions for box-shadow painting.

use super::*;

#[test]
fn box_shadow_bounds_cover_spread() {
    // #2143: spread expands the painted rect; culling must use the same geometry.
    let item = DisplayItem::BoxShadow {
        rect: Rect::new(-40.0, 0.0, 20.0, 20.0),
        radii: BorderRadii::default(),
        shadow: BoxShadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 50.0,
            color: Color::RED,
            inset: false,
        },
    };
    let bounds = item.bounds().expect("box shadow bounds");
    assert!(
        (bounds.x + 90.0).abs() < 0.001 && (bounds.width - 120.0).abs() < 0.001,
        "bounds must include spread, got {bounds:?}"
    );
}

#[test]
fn box_shadow_bounds_cover_three_pass_support() {
    let item = DisplayItem::BoxShadow {
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        radii: BorderRadii::default(),
        shadow: BoxShadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 64.0,
            spread: 0.0,
            color: Color::BLACK,
            inset: false,
        },
    };
    let bounds = item.bounds().expect("box shadow bounds");
    // #1716: three passes with radius blur/2 reach ~1.5 * blur, not blur.
    assert!(
        (bounds.x + 96.0).abs() < 0.001,
        "bounds must include the full blur support, got {bounds:?}"
    );
}
