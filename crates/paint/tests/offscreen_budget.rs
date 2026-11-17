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

#[test]
fn sibling_opacity_groups_release_their_budget() {
    // Siblings, not nesting: each group must return its reservation, or the 33rd group
    // (past the depth cap) would paint flat and lose its opacity.
    let mut items = Vec::new();
    for index in 0..33 {
        let rect = Rect::new(index as f32, 0.0, 1.0, 1.0);
        items.push(DisplayItem::OpacityGroup {
            rect,
            opacity: 0.5,
            items: vec![DisplayItem::SolidColor {
                rect,
                color: Color::new(255, 0, 0, 255),
            }],
        });
    }
    let pixmap = rasterize_display_list(&DisplayList { items }, 33, 1);
    for (index, pixel) in pixmap.pixels.iter().enumerate().take(33) {
        assert_eq!(
            (pixel.r, pixel.g, pixel.b),
            (255, 127, 127),
            "opacity group {index} lost its opacity (missing budget release?)"
        );
    }
}

#[test]
fn flatten_mode_keeps_embedded_opacity_content() {
    // 32 rounded groups fill the depth budget; the 33rd flattens. The opacity group inside
    // is embedded content: it must still paint (flatten mode used to drop it entirely).
    let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
    let mut items = Vec::new();
    for _ in 0..33 {
        items.push(DisplayItem::PushRoundedGroup {
            rect,
            radii: zero_radii(),
        });
    }
    items.push(DisplayItem::OpacityGroup {
        rect,
        opacity: 0.5,
        items: vec![DisplayItem::SolidColor {
            rect,
            color: Color::new(255, 0, 0, 255),
        }],
    });
    for _ in 0..33 {
        items.push(DisplayItem::PopRoundedGroup);
    }

    let pixmap = rasterize_display_list(&DisplayList { items }, 8, 8);
    let pixel = pixmap.pixels[2 * 8 + 2];
    // Flattening paints the contents without the layer (opacity is lost by design); what
    // matters is that the embedded content is not dropped (it used to leave white).
    assert_eq!(
        (pixel.r, pixel.g, pixel.b),
        (255, 0, 0),
        "the flattened group dropped its embedded opacity content"
    );
}

#[test]
fn deeply_nested_opacity_and_effect_groups_stay_bounded() {
    // #669: every nested EffectGroup/OpacityGroup allocates its own offscreen and the parents
    // stay alive, so without a budget a few hundred full-canvas groups become a memory DoS.
    // The budget caps each buffer, the live total and the nesting depth; past the depth cap
    // the contents are painted flat instead of recursing.
    let rect = Rect::new(0.0, 0.0, 64.0, 64.0);
    let depth = 300;
    // Each OpacityGroup is a container item, so build the nesting from the inside out.
    let mut nested = DisplayItem::SolidColor {
        rect,
        color: Color::new(0, 0, 255, 255),
    };
    for _ in 0..depth {
        nested = DisplayItem::OpacityGroup {
            rect,
            opacity: 0.98,
            items: vec![nested],
        };
    }
    let pixmap = rasterize_display_list(
        &DisplayList {
            items: vec![nested],
        },
        64,
        64,
    );
    let pixel = pixmap.pixels[32 * 64 + 32];
    assert!(
        pixel.b > 0,
        "the deeply nested content must still paint (flattened past the depth cap)"
    );

    // Effect groups go through the same budget: 300 nested `blur(0px)` groups must not
    // exhaust memory and must terminate.
    let filters = vec![css::FilterFunction::Blur(0.0)];
    let mut nested_effect = DisplayItem::SolidColor {
        rect,
        color: Color::new(0, 255, 0, 255),
    };
    for _ in 0..depth {
        nested_effect = DisplayItem::EffectGroup {
            rect,
            filters: filters.clone(),
            blend: css::MixBlendMode::Normal,
            items: vec![nested_effect],
        };
    }
    let pixmap = rasterize_display_list(
        &DisplayList {
            items: vec![nested_effect],
        },
        64,
        64,
    );
    assert!(
        pixmap.pixels[32 * 64 + 32].g > 0,
        "nested effect groups must still paint their content"
    );
}
