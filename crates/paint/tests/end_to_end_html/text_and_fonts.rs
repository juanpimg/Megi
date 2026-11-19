use super::support::*;

#[test]
fn test_real_font_glyphs_are_antialiased_and_baseline_aware() {
    // 32px black text on white, no margins: 'H' cap-height ink must start well
    // below the top of the text box (the old 8x8 stub started at row 0).
    let source = "<!doctype html><html><body style=\"margin:0;padding:0;color:#000;font-family:sans-serif;font-size:32px\"><p style=\"margin:0\">HH</p></body></html>";
    let (pixmap, display_list) = render(source, 240, 120);

    let (rect, baseline) = find_item(
        &display_list,
        |item| matches!(item, DisplayItem::Text { text, .. } if text == "HH"),
    )
    .map(|item| match item {
        DisplayItem::Text { rect, baseline, .. } => (*rect, *baseline),
        _ => unreachable!(),
    })
    .expect("expected a Text display item for HH");

    // Real metrics: ascent/size for DejaVu Sans is ~0.928em.
    assert!(
        baseline > 0.8 * 32.0,
        "baseline {baseline} must come from real ascent metrics"
    );

    let mut inked = 0u32;
    let mut antialiased = 0u32;
    let mut top_ink = u32::MAX;
    let x0 = rect.x.max(0.0) as u32;
    let y0 = rect.y.max(0.0) as u32;
    let x1 = (rect.x + rect.width).min(pixmap.width as f32) as u32;
    let y1 = (rect.y + rect.height).min(pixmap.height as f32) as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = pixmap.pixel(x, y);
            if pixel != Color::WHITE {
                inked += 1;
                top_ink = top_ink.min(y);
                if pixel.r > 0 && pixel.r < 200 {
                    antialiased += 1;
                }
            }
        }
    }
    assert!(inked > 20, "expected real glyph coverage, got {inked} px");
    assert!(
        antialiased > 0,
        "real rasterizer must produce antialiased (partial coverage) pixels"
    );
    assert!(
        top_ink >= y0 + 2,
        "glyph ink starts at row {top_ink} (text box top {y0}); a real 'H' leaves \
         ascent-cap-height whitespace above it, the old stub did not"
    );
}

/// An explicit but unavailable family (e.g. `Arial`) must paint with the real
/// bundled face, not the historical 8x8 bitmap stub (uppercase-only, no digits).
/// The rasterizer now appends the default generic to the family list (#1976).
#[test]
fn test_unknown_explicit_family_uses_real_glyphs() {
    let source = "<!doctype html><html><body style=\"margin:0;padding:0;color:#000;font-family:Arial;font-size:32px\"><p style=\"margin:0\">hello 123</p></body></html>";
    let (pixmap, display_list) = render(source, 400, 80);

    let mut inked = 0u32;
    let mut antialiased = 0u32;
    for item in display_list.iter() {
        if let DisplayItem::Text { rect, .. } = item {
            let x0 = rect.x.max(0.0) as u32;
            let y0 = rect.y.max(0.0) as u32;
            let x1 = (rect.x + rect.width).min(pixmap.width as f32) as u32;
            let y1 = (rect.y + rect.height).min(pixmap.height as f32) as u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let pixel = pixmap.pixel(x, y);
                    if pixel != Color::WHITE {
                        inked += 1;
                        if pixel.r > 0 && pixel.r < 250 {
                            antialiased += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        inked > 100,
        "expected real glyph ink for unknown explicit family, got {inked}"
    );
    assert!(
        antialiased > 10,
        "real bundled glyphs must be antialiased, got {antialiased} (8x8 stub?)"
    );
}

#[test]
fn test_text_baseline_is_propagated_per_line() {
    let source = "<!doctype html><html><body style=\"margin:0;color:#000;font-family:sans-serif;font-size:16px\"><p style=\"margin:0\">one<br>two</p></body></html>";
    let doc = megi_html::parse_document(source, false);
    let styles = resolve_embedded_styles(&doc);
    let viewport = Rect::new(0.0, 0.0, 200.0, 200.0);
    let measurer = megi_layout::FontTextMeasurer::bundled().expect("bundled fonts");
    let layout_root =
        megi_layout::layout_document_with_measurer(&doc, &styles, viewport, &measurer)
            .expect("layout root");
    let mut fragments = Vec::new();
    collect_text_baselines(&layout_root, &mut fragments);
    assert_eq!(fragments.len(), 2, "two lines => two text fragments");
    let metrics = measurer.font_metrics("sans-serif", 16.0, None);
    for (text, baseline, _y) in &fragments {
        assert!(
            (baseline - metrics.ascent).abs() <= 0.01,
            "fragment {text:?} baseline {baseline} must equal ascent {}",
            metrics.ascent
        );
    }
    // Absolute baseline positions differ by one line height.
    let metrics_line = measurer.font_metrics("sans-serif", 16.0, None);
    let absolute: Vec<f32> = fragments.iter().map(|(_, b, y)| y + b).collect();
    assert!(
        (absolute[1] - absolute[0] - metrics_line.line_height).abs() <= 0.01,
        "line baselines must advance by line_height {}: {:?}",
        metrics_line.line_height,
        absolute
    );
}

#[test]
fn test_render_is_byte_deterministic_across_runs() {
    let source = fixture("basic.html");
    let (first, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let (second, _) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    assert_eq!(
        first.to_rgba_bytes(),
        second.to_rgba_bytes(),
        "two renders of the same input must be byte-identical"
    );
}

#[test]
fn test_bold_text_uses_bold_face_and_has_baseline() {
    let source = fixture("basic.html");
    let (_pixmap, display_list) = render(&source, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    let bold = find_item(
        &display_list,
        |item| matches!(item, DisplayItem::Text { text, font_weight, .. } if text.contains("propagated") && *font_weight == 700),
    );
    assert!(
        bold.is_some(),
        "the .accent span must paint with font-weight 700 from the bold face"
    );
    if let Some(DisplayItem::Text { baseline, rect, .. }) = bold {
        assert!(*baseline > 0.0 && *baseline < rect.height);
    }
}
