use super::*;

#[test]
fn tab_strip_layout_distributes_and_reserves_buttons() {
    let titles: Vec<String> = (0..3).map(|index| format!("Tab {index}")).collect();
    let layout = layout_tabs(&titles, 640.0, 1);
    assert_eq!(layout.tabs.len(), 3);
    assert_eq!(layout.hidden, 0);
    for (index, tab) in layout.tabs.iter().enumerate() {
        assert_eq!(tab.index, index);
        assert!(tab.rect.width >= TAB_MIN_WIDTH - 0.1);
        assert!(tab.close.x >= tab.rect.x);
    }
    assert!(layout.new_tab.x >= layout.tabs.last().expect("tab").rect.x);
}

#[test]
fn tab_strip_hit_tests_activate_close_and_new() {
    let titles: Vec<String> = (0..2).map(|index| format!("Tab {index}")).collect();
    let layout = layout_tabs(&titles, 400.0, 0);
    let first = &layout.tabs[0];
    assert_eq!(
        tab_strip_hit(&layout, first.rect.x + 4.0, 10.0),
        Some(TabHit::Activate(0))
    );
    assert_eq!(
        tab_strip_hit(&layout, first.close.x + 2.0, first.close.y + 2.0),
        Some(TabHit::Close(0))
    );
    assert_eq!(
        tab_strip_hit(&layout, layout.new_tab.x + 2.0, 10.0),
        Some(TabHit::NewTab)
    );
    assert_eq!(tab_strip_hit(&layout, 10.0, TAB_STRIP_HEIGHT + 2.0), None);
}

#[test]
fn tab_strip_overflows_with_marker() {
    let titles: Vec<String> = (0..20).map(|index| format!("Tab {index}")).collect();
    let layout = layout_tabs(&titles, 300.0, 19);
    assert!(layout.hidden > 0, "expected hidden tabs");
    let overflow = layout.overflow.expect("overflow marker");
    assert!(overflow.width >= TAB_OVERFLOW_WIDTH - 0.1);
    assert_eq!(
        tab_strip_hit(&layout, overflow.x + 2.0, 10.0),
        Some(TabHit::Overflow)
    );
    assert!(!layout.active_visible);
}

#[test]
fn overflow_menu_lists_hidden_tabs_and_hit_tests_activate_and_close() {
    let titles: Vec<String> = (0..12).map(|index| format!("Tab {index}")).collect();
    let strip = layout_tabs(&titles, 300.0, 0);
    assert!(strip.hidden > 0);
    let hidden: Vec<usize> = (strip.tabs.len()..titles.len()).collect();
    let menu = OverflowMenu::open(hidden.clone());
    let layout = overflow_menu_layout(&strip, &titles, 300.0, 400, &menu);
    assert_eq!(
        layout.visible,
        menu.items.len().min(OVERFLOW_MENU_MAX_VISIBLE)
    );
    let first = layout.rows[0];
    assert_eq!(
        overflow_menu_hit(&layout, &menu, first.x + 4.0, first.y + 4.0),
        Some(OverflowHit::Activate(hidden[0]))
    );
    let close = layout.close[0];
    assert_eq!(
        overflow_menu_hit(&layout, &menu, close.x + 2.0, close.y + 2.0),
        Some(OverflowHit::Close(hidden[0]))
    );
    assert_eq!(overflow_menu_hit(&layout, &menu, 1.0, 1.0), None);
}

#[test]
fn overflow_menu_scrolls_and_hits_after_scrolling() {
    let titles: Vec<String> = (0..20).map(|index| format!("Tab {index}")).collect();
    let strip = layout_tabs(&titles, 260.0, 0);
    let hidden: Vec<usize> = (strip.tabs.len()..titles.len()).collect();
    let mut menu = OverflowMenu::open(hidden.clone());
    let before = overflow_menu_layout(&strip, &titles, 260.0, 400, &menu);
    assert!(menu.scroll_by(1));
    let after = overflow_menu_layout(&strip, &titles, 260.0, 400, &menu);
    assert_eq!(after.first, 1);
    let item = hidden[1];
    assert_eq!(
        overflow_menu_hit(&after, &menu, after.rows[0].x + 3.0, after.rows[0].y + 3.0),
        Some(OverflowHit::Activate(item))
    );
    assert_eq!(
        before.rows[0], after.rows[0],
        "menu geometry is stable when scrolling"
    );
    let _ = before;
}

#[test]
fn truncation_uses_an_ellipsis() {
    assert_eq!(truncate_title("abcdefghij", 5), "abcd…");
    assert_eq!(truncate_title("abc", 5), "abc");
    assert_eq!(truncate_title("", 5), "");
}
