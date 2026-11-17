//! Tabs: independent browsing state for several pages in one window (no native tab bar).
//!
//! Each [`Tab`] owns its own navigation history, document title/URL, scroll offset, page zoom
//! and find-in-page state. [`TabsState`] keeps a bounded list of tabs (`MAX_TABS`) and one
//! active index; \[`TabsState::next`\]/\[`TabsState::previous`\] wrap around and `Alt+1..9`
//! activates a concrete tab.
//!
//! **Closing policy** (documented, deterministic): closing a tab that is not the last one
//! keeps the active tab when the closed one was after it, shifts the active index down when
//! the closed one was before it, and activates the tab that followed the closed one (same
//! index, or the previous tab when the closed one was last) when it was active; it returns
//! [`CloseOutcome::Closed`].
//! Closing the **last remaining tab** does not close the window (the shell has no tab bar to
//! show a "reopen" affordance): the tab is reset to a blank **New Tab** document
//! ([`BLANK_TAB_URL`]) and [`CloseOutcome::ResetLast`] is returned.
//!
//! The window keeps exactly one live render state (one worker, generation-cancelled frames);
//! switching tabs copies the active tab's state back into its [`Tab`], loads the target tab
//! and requests a fresh render for its URL, so frames never leak between tabs. There is no
//! persistent session, no tab bar, no tab reordering and no per-tab process isolation yet.

use crate::find::FindState;
use crate::navigation::NavigationState;
use crate::scroll::ScrollState;
use crate::zoom;

/// Maximum number of open tabs.
pub const MAX_TABS: usize = 32;

/// Initial URL of a freshly opened tab (`Ctrl+T` / last-tab close): a blank data page.
pub const BLANK_TAB_URL: &str = "data:text/html,%3Ctitle%3ENew%20Tab%3C%2Ftitle%3E";

/// Fallback title of a blank tab that has not rendered yet.
pub const BLANK_TAB_TITLE: &str = "New Tab";

/// Result of [`TabsState::close`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseOutcome {
    /// The tab was removed; the active index was adjusted and remains valid.
    Closed,
    /// It was the last tab: it was reset to a blank New Tab instead of closing the window.
    ResetLast,
    /// The index did not reference an existing tab.
    Invalid,
}

/// One tab: its own history, URL, title, scroll, zoom and find state.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// Session history of this tab.
    pub navigation: NavigationState,
    /// Last document `<title>` seen in this tab.
    pub title: Option<String>,
    /// Current URL (normalized; equals the history entry unless a redirect is pending).
    pub url: String,
    /// Scroll offset and content/viewport heights for this tab.
    pub scroll: ScrollState,
    /// Page zoom factor of this tab (clamped 50–300 %).
    pub zoom: f64,
    /// Find-in-page state of this tab.
    pub find: FindState,
    /// Whether the find prompt was open when the tab was left.
    pub find_editing: bool,
    /// Last error shown in this tab, if any.
    pub error: Option<String>,
    /// Whether the last frame of this tab was clipped by the content cap.
    pub content_capped: bool,
    /// Presentation metadata of this tab's navigation history (titles/logical times).
    pub history_log: crate::history::HistoryLog,
    /// Last completed download name in this tab (in-memory only).
    pub last_download: Option<String>,
}

impl Tab {
    /// Creates a tab for `url`, which must be an absolute URL the window can render.
    pub fn new(url: impl Into<String>) -> Option<Self> {
        let url = url.into();
        let parsed = net::Url::parse(&url).ok()?;
        let mut navigation = NavigationState::new();
        navigation.navigate_to(parsed);
        Some(Self {
            navigation,
            title: None,
            url,
            scroll: ScrollState::default(),
            zoom: zoom::DEFAULT_ZOOM,
            find: FindState::new(),
            find_editing: false,
            error: None,
            content_capped: false,
            history_log: crate::history::HistoryLog::new(),
            last_download: None,
        })
    }

    /// Creates a blank **New Tab** page (used by `Ctrl+T` and the last-tab close policy).
    pub fn blank() -> Self {
        let mut tab = Self::new(BLANK_TAB_URL).unwrap_or_else(|| Self {
            navigation: NavigationState::new(),
            title: None,
            url: BLANK_TAB_URL.to_string(),
            scroll: ScrollState::default(),
            zoom: zoom::DEFAULT_ZOOM,
            find: FindState::new(),
            find_editing: false,
            error: None,
            content_capped: false,
            history_log: crate::history::HistoryLog::new(),
            last_download: None,
        });
        tab.title = Some(BLANK_TAB_TITLE.to_string());
        tab
    }

    /// Rebuilds a tab from persisted session state (history, URL, zoom, scroll).
    pub fn restored(
        navigation: NavigationState,
        url: String,
        zoom: f64,
        scroll: ScrollState,
    ) -> Self {
        Self {
            navigation,
            title: None,
            url,
            scroll,
            zoom: zoom::sanitize_zoom(zoom),
            find: FindState::new(),
            find_editing: false,
            error: None,
            content_capped: false,
            history_log: crate::history::HistoryLog::new(),
            last_download: None,
        }
    }

    /// Title used for the window title: document title, else the URL, else `New Tab`.
    pub fn display_title(&self) -> String {
        match self.title.as_deref() {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => match self.navigation.current() {
                Some(url) => url.as_str().to_string(),
                None => BLANK_TAB_TITLE.to_string(),
            },
        }
    }
}

/// Bounded, ordered collection of tabs plus the active index.
#[derive(Clone, Debug, PartialEq)]
pub struct TabsState {
    tabs: Vec<Tab>,
    active: usize,
}

impl TabsState {
    /// Creates a window with a single tab for `url` (invalid URLs produce one blank tab).
    pub fn new(url: impl Into<String>) -> Self {
        let tab = Tab::new(url).unwrap_or_else(Tab::blank);
        Self {
            tabs: vec![tab],
            active: 0,
        }
    }

    /// Creates the state for an initial tab list (empty falls back to a blank tab).
    pub fn from_urls(urls: Vec<String>) -> Self {
        let mut tabs: Vec<Tab> = urls
            .iter()
            .filter_map(|url| Tab::new(url.clone()))
            .collect();
        if tabs.is_empty() {
            tabs.push(Tab::blank());
        }
        tabs.truncate(MAX_TABS);
        Self { tabs, active: 0 }
    }

    /// Builds a tab state from restored tabs (caps the count and clamps the active index).
    pub fn from_parts(mut tabs: Vec<Tab>, active: usize) -> Self {
        if tabs.is_empty() {
            return Self::new(BLANK_TAB_URL);
        }
        tabs.truncate(MAX_TABS);
        let active = active.min(tabs.len() - 1);
        Self { tabs, active }
    }

    /// Number of open tabs (always at least 1).
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Whether there are no tabs. Never true for a live state (last-tab policy resets).
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Index of the active tab.
    pub const fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab.
    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// Mutable access to the active tab.
    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// All tabs, in order.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Whether the tab limit has been reached.
    pub fn is_full(&self) -> bool {
        self.tabs.len() >= MAX_TABS
    }

    /// Opens `url` as a new active tab; `None` when the limit is reached or the URL is bad.
    pub fn open_tab(&mut self, url: impl Into<String>) -> Option<usize> {
        if self.is_full() {
            return None;
        }
        let tab = Tab::new(url)?;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        Some(self.active)
    }

    /// Activates tab `index`; returns `false` for out-of-range indices.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active = index;
        true
    }

    /// Activates the next tab, wrapping around to the first one.
    pub fn next_tab(&mut self) -> usize {
        self.active = (self.active + 1) % self.tabs.len();
        self.active
    }

    /// Activates the previous tab, wrapping around to the last one.
    pub fn previous_tab(&mut self) -> usize {
        self.active = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.active
    }

    /// Closes the tab at `index` and applies the documented last-tab/neighbour policy.
    pub fn close(&mut self, index: usize) -> CloseOutcome {
        if index >= self.tabs.len() {
            return CloseOutcome::Invalid;
        }
        if self.tabs.len() == 1 {
            self.tabs[0] = Tab::blank();
            self.active = 0;
            return CloseOutcome::ResetLast;
        }
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        } else if index == self.active {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        CloseOutcome::Closed
    }

    /// Closes the active tab (see [`TabsState::close`]).
    pub fn close_active(&mut self) -> CloseOutcome {
        self.close(self.active)
    }

    /// Window title for a tab index: `"[2/3] title"` when several tabs are open, else `title`.
    pub fn indexed_title(index: usize, total: usize, title: &str) -> String {
        if total > 1 {
            format!("[{}/{}] {title}", index + 1, total)
        } else {
            title.to_string()
        }
    }

    /// Window title of the active tab, including its 1-based index when there are several.
    pub fn window_title(&self) -> String {
        Self::indexed_title(self.active, self.tabs.len(), &self.active().display_title())
    }
}

#[cfg(test)]
mod tests {
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
            state.active().navigation.current().map(net::Url::as_str),
            Some("https://example.com/".to_string())
        );
    }

    #[test]
    fn blank_tab_is_renderable_and_titled() {
        let tab = Tab::blank();
        assert_eq!(tab.url, BLANK_TAB_URL);
        assert_eq!(tab.display_title(), BLANK_TAB_TITLE);
        assert!(net::Url::parse(BLANK_TAB_URL).is_ok());
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
        assert_eq!(state.close_active(), CloseOutcome::ResetLast);
        assert_eq!(state.len(), 1);
        assert_eq!(state.active().url, BLANK_TAB_URL);
        assert_eq!(state.active().display_title(), BLANK_TAB_TITLE);
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
}
