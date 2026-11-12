//! Deeply nested offscreen groups must be bounded.
//!
//! 2,048 nested `border-radius` groups used to hold one full RGBA buffer and one stack
//! frame per level (≈3.9 GB for 800×600 groups; a stack overflow with small ones). The
//! painter now has a depth and a live-pixel budget; past it the contents are painted flat
//! and nested groups are skipped instead of recursing.

use css::{BorderRadii, Color, Length, Radius};
use layout::Rect;
use paint::{DisplayItem, DisplayList, rasterize_display_list};

const NESTING: usize = 2_048;

fn zero_radii() -> BorderRadii {
    let zero = Radius {
        x: Length::Px(0.0),
        y: Length::Px(0.0),
    };
    BorderRadii {
        top_left: zero,
        top_right: zero,
        bottom_right: zero,
        bottom_left: zero,
    }
}

#[test]
fn deeply_nested_rounded_groups_are_bounded() {
    let rect = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut items = Vec::with_capacity(NESTING * 2 + 1);
    for _ in 0..NESTING {
        items.push(DisplayItem::PushRoundedGroup {
            rect,
            radii: zero_radii(),
        });
    }
    items.push(DisplayItem::SolidColor {
        rect,
        color: Color::new(255, 0, 0, 255),
    });
    for _ in 0..NESTING {
        items.push(DisplayItem::PopRoundedGroup);
    }

    let pixmap = rasterize_display_list(&DisplayList { items }, 64, 64);
    let pixel = pixmap.pixels[32 * pixmap.width as usize + 32];
    assert_eq!(
        (pixel.r, pixel.g, pixel.b, pixel.a),
        (255, 0, 0, 255),
        "past the cap the contents are painted flat, not dropped"
    );
}
