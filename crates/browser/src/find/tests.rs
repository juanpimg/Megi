use super::*;

fn fixture_output() -> megi_webview::render::RenderOutput {
    let html = "<!DOCTYPE html><html><body>\
        <h1>Hello Find</h1>\
        <p>The quick brown fox jumps over the lazy dog. Quick QUICK quick.</p>\
        <p>second paragraph with QUICK again</p>\
        </body></html>";
    crate::headless::render_target(
        &format!("data:text/html,{}", html.replace(' ', "%20")),
        320,
        240,
    )
    .expect("render fixture")
}

fn position_of(text: &str, needle: &str) -> usize {
    text.find(needle).expect("needle present")
}

#[test]
fn search_is_case_insensitive_with_byte_ranges() {
    let output = fixture_output();
    let hits = search(&output.document, "quick");
    assert_eq!(
        hits.len(),
        5,
        "Quick, Quick, QUICK, quick + QUICK again/{hits:?}"
    );

    // Every range must point at the expected text, char-boundary aligned.
    for hit in &hits {
        let text = output
            .document
            .node(hit.node_id)
            .as_text()
            .expect("text node");
        assert_eq!(&text[hit.range.clone()].to_lowercase(), "quick");
    }
    let first = &hits[0];
    let text = output
        .document
        .node(first.node_id)
        .as_text()
        .expect("text node");
    assert_eq!(first.range.start, position_of(text, "quick"));
}

#[test]
fn search_multiple_occurrences_in_one_node() {
    let output = fixture_output();
    let hits = search(&output.document, "quick");
    let hamlet = hits
        .iter()
        .find(|hit| {
            output
                .document
                .node(hit.node_id)
                .as_text()
                .is_some_and(|text| text.contains("Quick QUICK quick"))
        })
        .expect("hamlet node");
    assert!(hamlet.range.end <= "x".len() + hamlet.range.start + 1024);
}

#[test]
fn search_without_matches_or_query_is_empty() {
    let output = fixture_output();
    assert!(search(&output.document, "megi-not-there").is_empty());
    assert!(search(&output.document, "").is_empty());
    assert!(
        search(&output.document, "   ").is_empty(),
        "spaces are searched literally"
    );
}

#[test]
fn find_state_next_and_previous_wrap_around() {
    let mut state = FindState::new();
    assert!(state.is_empty());
    assert!(!state.next_hit());
    assert!(!state.previous_hit());
    state.set_results(
        "quick",
        (0..3)
            .map(|index| FindMatch {
                node_id: NodeId::from_index(index),
                range: 0..1,
            })
            .collect(),
    );
    assert_eq!(state.current(), 0);
    assert!(state.next_hit());
    assert_eq!(state.current(), 1);
    assert!(state.next_hit());
    assert_eq!(state.current(), 2);
    assert!(state.next_hit());
    assert_eq!(state.current(), 0, "wraps to the first hit");
    assert!(state.previous_hit());
    assert_eq!(state.current(), 2, "wraps to the last hit");
    state.set_current(9);
    assert_eq!(state.current(), 2, "clamped");
    state.clear();
    assert!(state.is_empty());
    assert_eq!(state.query(), "");
}

#[test]
fn highlight_rects_follow_the_layout_fragments() {
    let output = fixture_output();
    let rects = highlight_rects(&output.layout, "quick");
    assert!(rects.len() >= 3, "one rect per text fragment: {rects:?}");
    for rect in &rects {
        assert!(rect.width > 0.0 && rect.height > 0.0);
        assert!(rect.y >= 0.0);
    }
    // A unique word in the first heading has a single rect.
    assert_eq!(highlight_rects(&output.layout, "Hello").len(), 1);
    assert!(highlight_rects(&output.layout, "zzz-not-found").is_empty());
}

#[test]
fn current_rect_uses_the_current_hit() {
    let output = fixture_output();
    let mut state = FindState::new();
    state.set_results("quick", search(&output.document, "quick"));
    let rect0 = state.current_rect(&output.layout).expect("first rect");
    assert!(state.next_hit());
    let rect1 = state.current_rect(&output.layout).expect("second rect");
    assert!(
        rect0.y <= rect1.y || rect0.x != rect1.x,
        "rect changes with the current hit"
    );
    state.set_results("nothing-here", Vec::new());
    assert!(state.current_rect(&output.layout).is_none());
}

#[test]
fn scroll_offset_centers_and_clamps() {
    let rect = CssRect::new(10.0, 400.0, 50.0, 20.0);
    assert_eq!(scroll_offset_for(&rect, 200, 1000), 310);
    assert_eq!(scroll_offset_for(&rect, 200, 100), 100);
    assert_eq!(
        scroll_offset_for(&CssRect::new(0.0, 0.0, 10.0, 10.0), 200, 1000),
        0
    );
    assert_eq!(scroll_offset_for(&rect, 200, 0), 0, "short page");
}

#[test]
fn pixmap_overlay_paints_yellow_and_stronger_current() {
    let output = fixture_output();
    let rects = highlight_rects(&output.layout, "quick");
    let mut pixmap = output.pixmap.clone();
    let rect = rects[0];
    let sample = ((rect.y + rect.height / 2.0) as usize) * (pixmap.width as usize)
        + (rect.x + rect.width / 2.0) as usize;
    let before = pixmap.pixels[sample];
    let painted = overlay_highlights_pixmap(&mut pixmap, &rects, 0);
    assert!(painted > 0);
    assert_ne!(pixmap.pixels[sample], before);
    // A regular (non-current) hit is blended at 0.45 over white; the current one at 0.75.
    let regular = blend_rgb((255, 255, 255), HIGHLIGHT_RGB, HIGHLIGHT_ALPHA);
    let current = blend_rgb((255, 255, 255), HIGHLIGHT_RGB, HIGHLIGHT_CURRENT_ALPHA);
    assert!(
        current.1 < regular.1,
        "current hit is more intense (yellow channel)"
    );
    let painted_current = pixmap
        .pixels
        .iter()
        .filter(|pixel| (pixel.r, pixel.g, pixel.b) == current)
        .count();
    assert!(painted_current > 0, "current highlight visible");
}

#[test]
fn dest_overlay_maps_visible_rects() {
    let mut dest = vec![0x00FF_FFFFu32; 20 * 10];
    let rect = CssRect::new(0.0, 8.0, 10.0, 4.0);
    // Page is 40 CSS px wide/20 tall; the frame is scrolled down 4 px.
    let painted = overlay_highlights_dest(&mut dest, 20, 10, 40, 20, 4, &[rect], 0);
    assert!(painted > 0);
    assert!(dest.iter().any(|pixel| *pixel != 0x00FF_FFFF));
    // A rect above the scroll window is clipped away.
    let offscreen = CssRect::new(0.0, 0.0, 10.0, 4.0);
    let mut untouched = vec![0x00FF_FFFFu32; 20 * 10];
    assert_eq!(
        overlay_highlights_dest(&mut untouched, 20, 10, 40, 20, 4, &[offscreen], 0),
        0
    );
    assert!(untouched.iter().all(|pixel| *pixel == 0x00FF_FFFF));
}
#[test]
fn search_ranges_cover_four_byte_scalars() {
    let document = megi_webview::html::parse_document("<body><p>a😀b</p></body>", false);
    let hits = search(&document, "😀");
    assert_eq!(hits.len(), 1);
    let text = document.node(hits[0].node_id).as_text().expect("text node");
    assert_eq!(&text[hits[0].range.clone()], "😀");
    assert!(text.is_char_boundary(hits[0].range.start));
    assert!(text.is_char_boundary(hits[0].range.end));
}

#[test]
fn search_ranges_stay_on_char_boundaries_when_lowercasing_expands() {
    // U+0130 (İ) lowercases to "i" + U+0307: a match of "i" ends inside the source
    // scalar, and the returned range must still be a char-boundary range.
    let document = megi_webview::html::parse_document("<body><p>İstanbul</p></body>", false);
    let hits = search(&document, "i");
    assert!(!hits.is_empty(), "the lowercased i matches");
    for hit in &hits {
        let text = document.node(hit.node_id).as_text().expect("text node");
        assert!(text.is_char_boundary(hit.range.start), "{:?}", hit.range);
        assert!(text.is_char_boundary(hit.range.end), "{:?}", hit.range);
    }
    let hit = &hits[0];
    let text = document.node(hit.node_id).as_text().expect("text node");
    assert!(
        text[hit.range.clone()].contains('İ'),
        "the range covers the whole source char: {:?}",
        hit.range
    );
}
