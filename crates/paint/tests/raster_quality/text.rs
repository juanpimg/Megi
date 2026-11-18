use super::common::*;

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
/// `megi_fonts::GlyphMask` placement offsets (`left`/`top`). Dropping them top-aligns
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
