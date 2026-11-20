
use super::*;

fn tab_state() -> TabsState {
    TabsState::from_urls(vec![
        "https://example.com/a".to_string(),
        "https://example.com/b".to_string(),
        "https://example.com/c".to_string(),
    ])
}

#[test]
fn new_state_has_one_active_tab() {
    let state = TabsState::new("https://example.com/");
    assert_eq!(state.len(), 1);
    assert_eq!(state.active_index(), 0);
    assert_eq!(state.active().url, "https://example.com/");
    assert_eq!(
        state
            .active()
            .navigation
            .current()
            .map(megi_webview::net::Url::as_str),
        Some("https://example.com/".to_string())
    );
}

#[test]
fn blank_tab_is_renderable_and_titled() {
    let tab = Tab::blank();
    let expected = crate::i18n::strings(crate::i18n::Locale::current()).new_tab;
    assert_eq!(tab.url, BLANK_TAB_URL);
    assert_eq!(tab.display_title(), expected);
    assert!(megi_webview::net::Url::parse(BLANK_TAB_URL).is_ok());
}

#[test]
fn open_tab_appends_and_activates_until_the_limit() {
    let mut state = TabsState::new("https://example.com/0");
    for index in 1..MAX_TABS {
        let opened = state.open_tab(format!("https://example.com/{index}"));
        assert_eq!(opened, Some(index));
    }
    assert_eq!(state.len(), MAX_TABS);
    assert!(state.is_full());
    assert_eq!(state.open_tab("https://example.com/overflow"), None);
    assert_eq!(state.active_index(), MAX_TABS - 1);
    assert_eq!(state.open_tab("not a url"), None);
}

#[test]
fn activate_next_and_previous_wrap_around() {
    let mut state = tab_state();
    assert!(state.activate(1));
    assert!(!state.activate(3));
    assert_eq!(state.next_tab(), 2);
    assert_eq!(state.next_tab(), 0, "wraps forward");
    assert_eq!(state.previous_tab(), 2, "wraps backward");
    assert_eq!(state.previous_tab(), 1);
}

#[test]
fn closing_the_last_tab_resets_it_to_blank() {
    let mut state = TabsState::new("https://example.com/only");
    let expected = crate::i18n::strings(crate::i18n::Locale::current()).new_tab;
    assert_eq!(state.close_active(), CloseOutcome::ResetLast);
    assert_eq!(state.len(), 1);
    assert_eq!(state.active().url, BLANK_TAB_URL);
    assert_eq!(state.active().display_title(), expected);
    assert_eq!(state.close(5), CloseOutcome::Invalid);
}

#[test]
fn closing_neighbours_keeps_a_valid_active_tab() {
    let mut state = tab_state();
    // Closing a tab before the active one shifts the active index down.
    state.activate(2);
    assert_eq!(state.close(0), CloseOutcome::Closed);
    assert_eq!(state.active_index(), 1);
    assert_eq!(state.active().url, "https://example.com/c");
    // Closing the active tab activates the neighbour at the same index (or the last).
    assert_eq!(state.close_active(), CloseOutcome::Closed);
    assert_eq!(state.len(), 1);
    assert_eq!(state.active_index(), 0);
    assert_eq!(state.active().url, "https://example.com/b");
}

#[test]
fn per_tab_state_is_independent() {
    let mut state = tab_state();
    {
        let tab = state.active_mut();
        tab.title = Some("Tab A".to_string());
        tab.scroll = ScrollState::new(2000, 400);
        tab.scroll.scroll_to(120);
        tab.zoom = 1.5;
        tab.find.set_results("needle", Vec::new());
        tab.find_editing = true;
        tab.error = Some("boom".to_string());
    }
    state.open_tab("https://example.com/b");
    assert_eq!(state.active().title, None);
    assert_eq!(state.active().zoom, 1.0);
    assert!(!state.active().find_editing);
    state.activate(0);
    assert_eq!(state.active().title.as_deref(), Some("Tab A"));
    assert_eq!(state.active().scroll.offset(), 120); // preserved per tab
    assert_eq!(state.active().zoom, 1.5);
    assert_eq!(state.active().find.query(), "needle");
    assert_eq!(state.active().error.as_deref(), Some("boom"));
    state.activate(1);
    assert_eq!(state.active().zoom, 1.0);
    assert_eq!(state.active().scroll.offset(), 0);
}

#[test]
fn indexed_title_prefixes_only_with_several_tabs() {
    assert_eq!(TabsState::indexed_title(0, 1, "Page — 100%"), "Page — 100%");
    assert_eq!(
        TabsState::indexed_title(1, 3, "Page — 100%"),
        "[2/3] Page — 100%"
    );
    let mut state = tab_state();
    assert_eq!(state.window_title(), "[1/3] https://example.com/a");
    state.next_tab();
    assert_eq!(state.window_title(), "[2/3] https://example.com/b");
}

#[test]
fn from_urls_skips_invalid_and_falls_back_to_blank() {
    let state = TabsState::from_urls(vec!["not a url".to_string()]);
    assert_eq!(state.len(), 1);
    assert_eq!(state.active().url, BLANK_TAB_URL);
    let mixed = TabsState::from_urls(vec![
        "not a url".to_string(),
        "data:text/html,x".to_string(),
        "file:///tmp/a.html".to_string(),
    ]);
    assert_eq!(mixed.len(), 2);
    assert_eq!(mixed.active().url, "data:text/html,x");
}

#[test]
fn blank_tab_title_uses_document_title_when_available() {
    let mut tab = Tab::blank();
    tab.title = Some("My Page".to_string());
    assert_eq!(tab.display_title(), "My Page");
    tab.title = Some(String::new());
    assert_eq!(tab.display_title(), BLANK_TAB_URL);
}

#[test]
fn single_tab_next_and_previous_stay_on_the_same_tab() {
    let mut state = TabsState::new("https://example.com/only");
    assert_eq!(state.next_tab(), 0);
    assert_eq!(state.previous_tab(), 0);
    assert_eq!(state.len(), 1);
}

#[test]
fn window_title_prefers_the_document_title() {
    let mut state = TabsState::new("https://example.com/a");
    state.active_mut().title = Some("Document".to_string());
    state.open_tab("https://example.com/b");
    assert_eq!(state.window_title(), "[2/2] https://example.com/b");
    state.activate(0);
    assert_eq!(state.window_title(), "[1/2] Document");
}
#[test]
fn closing_the_active_middle_tab_activates_the_next_one() {
    // Documented policy: the tab that followed the closed one takes its index.
    let mut state = tab_state();
    assert_eq!(state.len(), 3);
    assert!(state.activate(1));
    assert_eq!(state.close_active(), CloseOutcome::Closed);
    assert_eq!(state.len(), 2);
    assert_eq!(state.active_index(), 1);
    assert_eq!(state.active().url, "https://example.com/c");
}

#[test]
fn tabs_report_distinct_isolation_sites() {
    // ADR-004 phase 1: same-site tabs share a renderer group, cross-site tabs do not.
    let one_site = TabsState::from_urls(vec![
        "https://example.com/a".to_owned(),
        "https://example.com/b".to_owned(),
    ]);
    assert_eq!(one_site.distinct_sites(), 1);
    assert!(!one_site.has_cross_site_tabs());

    let two_sites = TabsState::from_urls(vec![
        "https://example.com/a".to_owned(),
        "http://example.com/b".to_owned(),
        "https://other.test/".to_owned(),
    ]);
    assert_eq!(two_sites.distinct_sites(), 2);
    assert!(two_sites.has_cross_site_tabs());
}
