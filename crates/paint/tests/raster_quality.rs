//! Rasterizer quality tests for WP-4.11 + WP-4.3:
//!
//! - antialiasing of fractional rectangle edges (and no round-out over-paint),
//! - per-side borders (colors, styles, mitred corners),
//! - clip semantics and safe `pop_clip`,
//! - text-cache telemetry (shaped runs and glyph masks are reused),
//! - culling of off-screen items,
//! - `Pixmap` dimension validation.

use css::{BorderStyle, Color};
use layout::Rect;
use paint::{
    BorderSide, DisplayItem, DisplayList, Pixmap, RenderConfig,
    rasterize_display_list_with_font_database, rasterize_display_list_with_font_database_and_stats,
};

/// Bundled font database used by the reference pipeline.
fn database() -> &'static fonts::FontDatabase {
    paint::bundled_font_database()
}

/// A red border side of the given width.
fn side(width: f32, style: BorderStyle, color: Color) -> BorderSide {
    BorderSide::new(width, style, color)
}

/// Counts how much red ink a white-canvas pixmap contains: `sum(255 - g)` over
/// all pixels equals `255 * covered_area` because a pure-red source at alpha
/// `a` leaves `g = 255 * (1 - a)` on white.
fn red_ink_mass(pixmap: &Pixmap) -> f64 {
    pixmap
        .pixels
        .iter()
        .map(|pixel| f64::from(255 - pixel.g))
        .sum::<f64>()
        / 255.0
}

#[test]
fn test_fill_rect_antialiases_fractional_edges() {
    let mut pixmap = Pixmap::new_with_background(4, 1, Color::WHITE);
    pixmap.fill_rect(Rect::new(1.5, 0.0, 2.0, 1.0), Color::RED);

    // Column 0 is untouched; columns 1 and 3 are half covered; column 2 is full.
    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
    let left = pixmap.pixel(1, 0);
    let middle = pixmap.pixel(2, 0);
    let right = pixmap.pixel(3, 0);
    assert_eq!(middle, Color::RED);
    for edge in [left, right] {
        assert_eq!(edge.r, 255);
        assert_eq!(edge.a, 255);
        assert!(
            (120..=135).contains(&edge.g),
            "half-covered edge should blend ~50 % red over white, got {edge:?}"
        );
        assert_eq!(edge.g, edge.b);
    }
}

#[test]
fn test_fill_rect_fractional_corner_has_quarter_coverage() {
    let mut pixmap = Pixmap::new_with_background(3, 3, Color::WHITE);
    pixmap.fill_rect(Rect::new(1.5, 1.5, 1.0, 1.0), Color::RED);

    // A 1x1 rect straddling the (1.5, 1.5) corner covers a quarter of each of
    // the four pixels around it.
    for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
        let corner = pixmap.pixel(x, y);
        assert!(
            (180..=205).contains(&corner.g),
            "quarter-covered pixel ({x}, {y}) should be ~25 % red, got {corner:?}"
        );
    }
    assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
    assert_eq!(pixmap.pixel(2, 0), Color::WHITE);
}

#[test]
fn test_fractional_rect_area_is_not_overpainted() {
    // Classic finding F-13: fill_rect(2.5, 2.5, 5, 5) painted 36 px instead of
    // the exact 25 px area because of floor/ceil round-out.
    let mut pixmap = Pixmap::new_with_background(12, 12, Color::WHITE);
    pixmap.fill_rect(Rect::new(2.5, 2.5, 5.0, 5.0), Color::RED);
    let covered = red_ink_mass(&pixmap);
    assert!(
        (covered - 25.0).abs() < 1.0,
        "fractional rect must cover its 25 px² area, got {covered:.2} px²"
    );
}

#[test]
fn test_integer_rect_is_still_exactly_opaque() {
    let mut pixmap = Pixmap::new_with_background(8, 8, Color::WHITE);
    pixmap.fill_rect(Rect::new(2.0, 3.0, 4.0, 2.0), Color::RED);
    for y in 3..5 {
        for x in 2..6 {
            assert_eq!(pixmap.pixel(x, y), Color::RED, "pixel ({x}, {y})");
        }
    }
    assert_eq!(pixmap.pixel(1, 3), Color::WHITE);
    assert_eq!(pixmap.pixel(6, 3), Color::WHITE);
}

#[test]
fn test_border_sides_use_their_own_colors() {
    let top = Color::new(255, 0, 0, 255);
    let right = Color::new(0, 255, 0, 255);
    let bottom = Color::new(0, 0, 255, 255);
    let left = Color::new(255, 255, 0, 255);
    // border box at (10, 10), 100x60; sides: top 4, right 6, bottom 2, left 8.
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(10.0, 10.0, 100.0, 60.0),
        sides: [
            side(4.0, BorderStyle::Solid, top),
            side(6.0, BorderStyle::Solid, right),
            side(2.0, BorderStyle::Solid, bottom),
            side(8.0, BorderStyle::Solid, left),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        120,
        80,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    assert_eq!(pixmap.pixel(50, 11), top, "top band");
    assert_eq!(pixmap.pixel(106, 40), right, "right band");
    assert_eq!(pixmap.pixel(50, 69), bottom, "bottom band");
    assert_eq!(pixmap.pixel(13, 40), left, "left band");
    // Inside the border box is untouched (transparent border interior).
    assert_eq!(pixmap.pixel(60, 40), Color::WHITE);
}

#[test]
fn test_border_corners_are_mitred() {
    let top = Color::new(255, 0, 0, 255);
    let left = Color::new(255, 255, 0, 255);
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(10.0, 10.0, 100.0, 60.0),
        sides: [
            side(8.0, BorderStyle::Solid, top),
            BorderSide::NONE,
            BorderSide::NONE,
            side(8.0, BorderStyle::Solid, left),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        120,
        80,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    // Top-left corner square is 8x8 at (10, 10); the diagonal goes from (10,10)
    // to (18,18). Above/right of it is top (red), below/left is left (yellow).
    assert_eq!(pixmap.pixel(16, 11), top, "upper-right of the mitre is top");
    assert_eq!(
        pixmap.pixel(11, 16),
        left,
        "lower-left of the mitre is left"
    );
    // The diagonal pixels are antialiased blends of both colors.
    let diagonal = pixmap.pixel(14, 14);
    assert_ne!(diagonal, top);
    assert_ne!(diagonal, left);
    assert!(
        diagonal.r == 255 && diagonal.g > 0 && diagonal.g < 255,
        "diagonal pixel must mix red and yellow, got {diagonal:?}"
    );
}

#[test]
fn test_border_styles_dash_dot_double() {
    let red = Color::RED;
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(0.0, 0.0, 60.0, 20.0),
        sides: [
            side(2.0, BorderStyle::Dashed, red),
            BorderSide::NONE,
            side(2.0, BorderStyle::Dotted, red),
            BorderSide::NONE,
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        60,
        20,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    // Dashed top band: painted runs interrupted by gaps.
    let row: Vec<bool> = (0..60).map(|x| pixmap.pixel(x, 1) == red).collect();
    let painted = row.iter().filter(|painted| **painted).count();
    assert!(painted > 0 && painted < 60, "dashed top must have gaps");
    let runs = row.windows(2).filter(|pair| !pair[0] && pair[1]).count();
    assert!(runs >= 2, "dashed top must have at least two dashes");

    // Dotted bottom band: small dots separated by larger gaps.
    let dot_row: Vec<bool> = (0..60).map(|x| pixmap.pixel(x, 18) == red).collect();
    let dot_painted = dot_row.iter().filter(|painted| **painted).count();
    assert!(
        dot_painted > 0 && dot_painted < 30,
        "dotted bottom has dots"
    );

    // Double: two thin lines with a gap, on a 6 px top border.
    let mut double_list = DisplayList::new();
    double_list.push(DisplayItem::Border {
        rect: Rect::new(0.0, 0.0, 40.0, 20.0),
        sides: [
            side(6.0, BorderStyle::Double, red),
            BorderSide::NONE,
            BorderSide::NONE,
            BorderSide::NONE,
        ],
    });
    let double = rasterize_display_list_with_font_database(
        &double_list,
        40,
        20,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert_eq!(double.pixel(20, 1), red, "first line of double border");
    assert_eq!(double.pixel(20, 3), Color::WHITE, "gap of double border");
    assert_eq!(double.pixel(20, 5), red, "second line of double border");
}

#[test]
fn test_border_none_and_hidden_paint_nothing() {
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Border {
        rect: Rect::new(5.0, 5.0, 30.0, 20.0),
        sides: [
            side(4.0, BorderStyle::None, Color::RED),
            side(4.0, BorderStyle::Hidden, Color::BLUE),
            side(4.0, BorderStyle::None, Color::GREEN),
            side(4.0, BorderStyle::Hidden, Color::BLACK),
        ],
    });
    let pixmap = rasterize_display_list_with_font_database(
        &display_list,
        40,
        30,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert!(
        pixmap.pixels.iter().all(|pixel| *pixel == Color::WHITE),
        "border styles none/hidden must not paint"
    );
}

#[test]
fn test_clip_limits_fill_and_set_pixel() {
    let mut pixmap = Pixmap::new_with_background(20, 20, Color::WHITE);
    pixmap.push_clip(Rect::new(5.0, 5.0, 10.0, 10.0));
    pixmap.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::RED);
    for y in 5..15 {
        for x in 5..15 {
            assert_eq!(pixmap.pixel(x, y), Color::RED, "inside clip ({x}, {y})");
        }
    }
    assert_eq!(pixmap.pixel(4, 10), Color::WHITE, "outside clip");
    assert_eq!(pixmap.pixel(10, 15), Color::WHITE, "outside clip");

    pixmap.set_pixel(2, 2, Color::BLUE);
    assert_eq!(
        pixmap.pixel(2, 2),
        Color::WHITE,
        "set_pixel must respect clip"
    );
    pixmap.set_pixel(6, 6, Color::BLUE);
    assert_eq!(pixmap.pixel(6, 6), Color::BLUE);

    pixmap.pop_clip();
    pixmap.fill_rect(Rect::new(0.0, 0.0, 20.0, 20.0), Color::GREEN);
    assert_eq!(pixmap.pixel(2, 2), Color::GREEN, "clip popped");
}

#[test]
fn test_pop_clip_on_empty_stack_is_safe() {
    let mut pixmap = Pixmap::new_with_background(4, 4, Color::WHITE);
    pixmap.pop_clip();
    pixmap.pop_clip();
    pixmap.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::RED);
    assert_eq!(pixmap.pixel(0, 0), Color::RED);
}

#[test]
fn test_pixmap_rejects_absurd_dimensions() {
    // Oversized dimensions yield an empty pixmap instead of panicking (issue #667).
    let pixmap = Pixmap::new(u32::MAX, u32::MAX);
    assert_eq!(pixmap.width, 0);
    assert_eq!(pixmap.height, 0);
    assert!(pixmap.pixels.is_empty());
}

#[test]
fn test_text_cache_shapes_and_rasterizes_each_glyph_once() {
    let mut display_list = DisplayList::new();
    for _ in 0..3 {
        display_list.push(DisplayItem::Text {
            text: "aabbcc".to_string(),
            rect: Rect::new(0.0, 0.0, 60.0, 20.0),
            color: Color::BLACK,
            font_size: 16.0,
            font_family: "sans-serif".to_string(),
            font_weight: 400,
            font_italic: false,
            advance: 10.0,
            baseline: 14.0,
        });
    }

    let (pixmap, stats) = rasterize_display_list_with_font_database_and_stats(
        &display_list,
        80,
        24,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );

    assert_eq!(stats.text_items, 3);
    assert_eq!(stats.shaped_runs, 1, "one distinct shaped run");
    assert_eq!(stats.shaped_run_hits, 2, "two cache hits for repeated runs");
    assert_eq!(stats.glyph_masks_rasterized, 3, "a, b and c masks only");
    assert!(
        stats.glyph_mask_hits >= 15,
        "repeated glyphs must hit the mask cache, got {}",
        stats.glyph_mask_hits
    );
    assert_eq!(stats.glyphs_drawn, 18);

    // Telemetry is side-effect free: the pixels match a plain render.
    let plain = rasterize_display_list_with_font_database(
        &display_list,
        80,
        24,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert_eq!(pixmap.to_rgba_bytes(), plain.to_rgba_bytes());
}

#[test]
fn test_offscreen_text_is_culled() {
    let mut display_list = DisplayList::new();
    display_list.push(DisplayItem::Text {
        text: "hidden".to_string(),
        rect: Rect::new(0.0, 5000.0, 60.0, 20.0),
        color: Color::BLACK,
        font_size: 16.0,
        font_family: "sans-serif".to_string(),
        font_weight: 400,
        font_italic: false,
        advance: 10.0,
        baseline: 14.0,
    });
    let (pixmap, stats) = rasterize_display_list_with_font_database_and_stats(
        &display_list,
        80,
        24,
        RenderConfig::OPAQUE_WHITE,
        database(),
    );
    assert_eq!(stats.text_items, 0, "off-screen text must be culled");
    assert!(pixmap.pixels.iter().all(|pixel| *pixel == Color::WHITE));
}

/// Regression test (round 3): glyph masks must be placed with the
/// `fonts::GlyphMask` placement offsets (`left`/`top`). Dropping them top-aligns
/// every glyph's ink box, which lifts periods and dots to cap height and
/// shifts text horizontally.
#[test]
fn test_glyph_masks_are_placed_on_the_baseline_not_top_aligned() {
    let baseline = 60.0f32;
    let dl = DisplayList {
        items: vec![DisplayItem::Text {
            text: "A.".to_string(),
            rect: Rect::new(0.0, 0.0, 120.0, 100.0),
            color: Color::BLACK,
            font_size: 64.0,
            font_family: "sans-serif".to_string(),
            font_weight: 400,
            font_italic: false,
            advance: 0.0,
            baseline,
        }],
    };
    let pixmap = rasterize_display_list_with_font_database(
        &dl,
        120,
        100,
        RenderConfig::default(),
        database(),
    );

    // Group ink columns into glyph runs (A, then the period).
    let ink = |x: u32, y: u32| pixmap.pixel(x, y).r < 128;
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for x in 0..120u32 {
        let has_ink = (0..100u32).any(|y| ink(x, y));
        if !has_ink {
            continue;
        }
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == x => *end = x,
            _ => runs.push((x, x)),
        }
    }
    assert_eq!(runs.len(), 2, "expected two glyph ink runs, got {runs:?}");

    let row_span = |(x0, x1): (u32, u32)| {
        let rows: Vec<u32> = (0..100u32)
            .filter(|&y| (x0..=x1).any(|x| ink(x, y)))
            .collect();
        (*rows.first().unwrap(), *rows.last().unwrap())
    };
    let (_, a_bottom) = row_span(runs[0]);
    let (period_top, period_bottom) = row_span(runs[1]);

    // 'A' is a capital letter: it ends on the baseline.
    assert!(
        (a_bottom as f32 - baseline).abs() <= 2.0,
        "'A' should sit on the baseline {baseline}, ink ends at {a_bottom}"
    );
    // A period is a small dot near the baseline, not a raised dot at cap height.
    assert!(
        period_bottom as f32 >= baseline - 8.0,
        "period ink should reach the baseline (bottom={period_bottom}, baseline={baseline})"
    );
    assert!(
        period_top as f32 >= baseline - 20.0,
        "period must not be lifted to cap height (top={period_top}, baseline={baseline})"
    );
}

// --- Regression tests for the fractional-width coverage bug (R46) -----------
//
// The optimized `fill_rect` path splits the row into "partial columns + fully
// covered columns + partial columns". When a rectangle is narrower than one
// pixel (or has fractional x and no fully covered column), `full_begin` and
// `full_end` collapsed to 0 and the trailing partial loop indexed
// `coverage_x[x - ix0]` for x < ix0: debug builds panicked with a subtract
// overflow, release builds indexed out of bounds. These tests fail (panic)
// before the fix and assert exact antialiased coverage after it.

#[test]
fn test_fill_rect_subpixel_width_full_rows_is_antialiased() {
    let mut pixmap = Pixmap::new_with_background(16, 4, Color::WHITE);
    pixmap.fill_rect(Rect::new(10.3, 0.0, 0.4, 4.0), Color::RED);
    for y in 0..4 {
        let covered = pixmap.pixel(10, y);
        assert_eq!(covered.r, 255);
        assert!(
            (140..=170).contains(&covered.g),
            "column 10 should be ~40 % red, got {covered:?}"
        );
        assert_eq!(pixmap.pixel(9, y), Color::WHITE);
        assert_eq!(pixmap.pixel(11, y), Color::WHITE);
    }
}

#[test]
fn test_fill_rect_one_pixel_with_fractional_x_and_full_rows() {
    // Columns 5 and 6 are each half covered; rows 1..3 are fully covered, which
    // is exactly the code path with `coverage_y == 256` and no full column.
    let mut pixmap = Pixmap::new_with_background(12, 6, Color::WHITE);
    pixmap.fill_rect(Rect::new(5.5, 1.0, 1.0, 2.0), Color::RED);
    for y in 1..3 {
        for x in [5u32, 6u32] {
            let pixel = pixmap.pixel(x, y);
            assert_eq!(pixel.r, 255);
            assert!(
                (115..=140).contains(&pixel.g),
                "column {x} row {y} should be ~50 % red, got {pixel:?}"
            );
        }
    }
    assert_eq!(pixmap.pixel(4, 1), Color::WHITE);
    assert_eq!(pixmap.pixel(7, 1), Color::WHITE);
    assert_eq!(pixmap.pixel(5, 0), Color::WHITE);
}

#[test]
fn test_fill_rect_partial_full_partial_area_is_exact() {
    // x = 5.5, width 3: column 5 is half, column 6 is fully covered, column 7
    // is half. Total covered area must stay exactly 3 x 4 = 12 px².
    let mut pixmap = Pixmap::new_with_background(16, 4, Color::WHITE);
    pixmap.fill_rect(Rect::new(5.5, 0.0, 3.0, 4.0), Color::RED);
    for y in 0..4 {
        assert_eq!(pixmap.pixel(6, y), Color::RED, "fully covered column");
    }
    let covered = red_ink_mass(&pixmap);
    assert!(
        (covered - 12.0).abs() < 0.5,
        "mixed partial/full rect must cover 12 px², got {covered:.2}"
    );
}

#[test]
fn test_fill_rect_degenerate_and_non_finite_do_not_paint() {
    let mut pixmap = Pixmap::new_with_background(8, 8, Color::WHITE);
    for rect in [
        Rect::new(2.0, 2.0, 0.0, 5.0),
        Rect::new(2.0, 2.0, 5.0, 0.0),
        Rect::new(2.0, 2.0, -1.0, -1.0),
        Rect::new(f32::NAN, 0.0, 1.0, 1.0),
        Rect::new(0.0, f32::INFINITY, 1.0, 1.0),
    ] {
        pixmap.fill_rect(rect, Color::RED);
    }
    assert!(
        pixmap.pixels.iter().all(|pixel| *pixel == Color::WHITE),
        "degenerate/non-finite rectangles must not paint"
    );
}
