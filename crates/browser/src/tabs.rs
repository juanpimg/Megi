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
//!
//! Local workspaces ([`Workspace`]) group tabs in memory: only the active group is visible
//! in the rail/tab strip, and `Ctrl+Alt+W` cycles groups without touching their tab state.
//! With a single workspace all indices and painted bytes stay identical to the legacy path;
//! persistence of the workspace assignment is deferred to phase 5.

use crate::find::FindState;
use crate::navigation::NavigationState;
use crate::scroll::ScrollState;
use crate::zoom;

mod workspaces;

pub use workspaces::{MAX_WORKSPACES, Workspace};

/// Maximum number of open tabs.
pub const MAX_TABS: usize = 32;

/// Initial URL of a freshly opened tab (`Ctrl+T` / last-tab close): a blank data page.
pub const BLANK_TAB_URL: &str = "data:text/html,%3Ctitle%3ENew%20Tab%3C%2Ftitle%3E";

/// Fallback title of a blank tab that has not rendered yet.
pub const BLANK_TAB_TITLE: &str = "New Tab";

fn blank_tab_title() -> &'static str {
    crate::i18n::strings(crate::i18n::Locale::current()).new_tab
}

/// Result of closing a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseOutcome {
    /// The tab was removed; the active index was adjusted and remains valid.
    Closed,
    /// It was the last tab of its workspace: reset to blank instead of emptying the group.
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
    /// Local workspace id (`0..MAX_WORKSPACES`); other groups stay open but hidden.
    pub workspace: u8,
}

impl Tab {
    /// Creates a tab for `url`, which must be an absolute URL the window can render.
    pub fn new(url: impl Into<String>) -> Option<Self> {
        let url = url.into();
        let parsed = megi_webview::net::Url::parse(&url).ok()?;
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
            workspace: 0,
        })
    }

    /// Creates a blank **New Tab** page (used by `Ctrl+T` and the last-tab close policy).
    pub(crate) fn blank() -> Self {
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
            workspace: 0,
        });
        tab.title = Some(blank_tab_title().to_string());
        tab
    }

    /// Rebuilds a tab from persisted session state (history, URL, zoom, scroll).
    pub(crate) fn restored(
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
            workspace: 0,
        }
    }

    /// Title used for the window title: document title, else the URL, else the localized blank-tab title.
    pub fn display_title(&self) -> String {
        match self.title.as_deref() {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => title_fallback(self),
        }
    }
}

/// Fallback title of a tab that has no document title yet.
fn title_fallback(tab: &Tab) -> String {
    match tab.navigation.current() {
        Some(url) => url.as_str().to_string(),
        None => blank_tab_title().to_string(),
    }
}

/// Owned tab data for callers that persist or summarize tabs without borrowing them.
#[derive(Clone, Debug, PartialEq)]
pub struct TabSnapshot {
    pub url: String,
    pub history: Vec<String>,
    pub index: usize,
    pub zoom: f64,
    pub scroll: u32,
    pub workspace: u8,
}

/// Parses the URL list into tabs, dropping bad URLs and capping the count.
fn tabs_from_urls(urls: &[String]) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = urls
        .iter()
        .filter_map(|url| Tab::new(url.clone()))
        .collect();
    tabs.truncate(MAX_TABS);
    tabs
}

/// Deduplicated isolation-site keys of a tab list, in tab order.
fn distinct_site_keys(tabs: &[Tab]) -> Vec<crate::site::SiteKey> {
    let mut keys: Vec<crate::site::SiteKey> = Vec::new();
    for tab in tabs {
        let key = crate::site::site_key(&tab.url);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Display titles of the tabs, in stored order.
fn display_titles(tabs: &[Tab]) -> Vec<String> {
    tabs.iter().map(Tab::display_title).collect()
}

/// Workspace ids of the tabs, in stored order.
fn workspace_ids(tabs: &[Tab]) -> Vec<u8> {
    tabs.iter().map(|tab| tab.workspace).collect()
}

/// Clamps restored workspace ids so every tab stays in a valid group.
fn clamp_workspace_ids(tabs: &mut [Tab]) {
    for tab in tabs {
        if usize::from(tab.workspace) >= usize::from(MAX_WORKSPACES) {
            tab.workspace = MAX_WORKSPACES - 1;
        }
    }
}

/// Bounded, ordered collection of tabs plus the active index.
#[derive(Clone, Debug, PartialEq)]
pub struct TabsState {
    tabs: Vec<Tab>,
    active: usize,
    workspace: u8,
    workspace_active: [Option<usize>; MAX_WORKSPACES as usize],
}

impl TabsState {
    /// Creates a window with a single tab for `url` (invalid URLs produce one blank tab).
    pub fn new(url: impl Into<String>) -> Self {
        let tab = Tab::new(url).unwrap_or_else(Tab::blank);
        Self {
            tabs: vec![tab],
            active: 0,
            workspace: 0,
            workspace_active: workspaces::initial_slots(),
        }
    }

    /// Creates the state for an initial tab list (empty falls back to a blank tab).
    pub fn from_urls(urls: Vec<String>) -> Self {
        let mut tabs = tabs_from_urls(&urls);
        if tabs.is_empty() {
            tabs.push(Tab::blank());
        }
        Self {
            tabs,
            active: 0,
            workspace: 0,
            workspace_active: workspaces::initial_slots(),
        }
    }

    /// Number of distinct effective sites across open tabs (ADR-004 phase 1).
    ///
    /// Tabs on the same site may share a renderer; tabs on different sites must not.
    fn distinct_sites(&self) -> usize {
        distinct_site_keys(&self.tabs).len()
    }

    /// True when the window holds tabs from more than one isolation site (ADR-004).
    pub fn has_cross_site_tabs(&self) -> bool {
        self.distinct_sites() > 1
    }

    /// Builds a tab state from restored tabs (caps the count and clamps the active index).
    pub(crate) fn from_parts(mut tabs: Vec<Tab>, active: usize) -> Self {
        if tabs.is_empty() {
            return Self::new(BLANK_TAB_URL);
        }
        tabs.truncate(MAX_TABS);
        // Restored tabs have no persisted workspace id yet (deferred to phase 5):
        // clamp any out-of-range id so the workspace invariants always hold.
        clamp_workspace_ids(&mut tabs);
        let active = active.min(tabs.len() - 1);
        let workspace_active = workspaces::rebuild_slots(&tabs, active);
        let workspace = tabs[active].workspace;
        Self {
            tabs,
            active,
            workspace,
            workspace_active,
        }
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
    pub(crate) fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// URLs in tab order, for callers that need to locate a tab without borrowing it.
    pub(crate) fn url_labels(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| tab.url.clone()).collect()
    }

    /// Display titles and workspace indexes, in tab order, for chrome painting.
    pub fn rail_labels(&self) -> (Vec<String>, Vec<u8>) {
        (display_titles(&self.tabs), workspace_ids(&self.tabs))
    }

    /// Owned copy of one tab's persistence fields.
    fn snapshot_of(tab: &Tab) -> TabSnapshot {
        TabSnapshot {
            url: tab.url.clone(),
            history: tab
                .navigation
                .history()
                .iter()
                .map(|url| url.as_str().to_owned())
                .collect(),
            index: tab.navigation.index(),
            zoom: tab.zoom,
            scroll: tab.scroll.offset(),
            workspace: tab.workspace,
        }
    }

    /// Owned snapshots of up to `limit` leading tabs, for session persistence.
    pub(crate) fn snapshots(&self, limit: usize) -> Vec<TabSnapshot> {
        self.tabs
            .iter()
            .take(limit)
            .map(Self::snapshot_of)
            .collect()
    }

    /// Whether the tab limit has been reached.
    pub(crate) fn is_full(&self) -> bool {
        self.tabs.len() >= MAX_TABS
    }

    /// Opens `url` as a new active tab; `None` when the limit is reached or the URL is bad.
    pub fn open_tab(&mut self, url: impl Into<String>) -> Option<usize> {
        if self.is_full() {
            return None;
        }
        let workspace = self.workspace;
        let mut tab = Tab::new(url)?;
        tab.workspace = workspace;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.workspace_active[usize::from(workspace)] = Some(self.active);
        Some(self.active)
    }

    /// Activates tab `index`; returns `false` for out-of-range indices.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.note_active(index);
        true
    }

    /// Activates the next tab of the active workspace, wrapping to its first one.
    pub(crate) fn next_tab(&mut self) -> usize {
        self.step_tab(true)
    }

    /// Activates the previous tab of the active workspace, wrapping to its last one.
    pub(crate) fn previous_tab(&mut self) -> usize {
        self.step_tab(false)
    }

    /// Closes the tab at `index` and applies the documented last-tab/neighbour policy.
    pub(crate) fn close(&mut self, index: usize) -> CloseOutcome {
        self.close_tab(index)
    }

    /// Closes the active tab (see [`TabsState::close`]).
    pub(crate) fn close_active(&mut self) -> CloseOutcome {
        self.close(self.active)
    }

    /// Window title for a tab index: `"[2/3] title"` when several tabs are open, else `title`.
    pub(crate) fn indexed_title(index: usize, total: usize, title: &str) -> String {
        if total > 1 {
            format!("[{}/{}] {title}", index + 1, total)
        } else {
            title.to_string()
        }
    }

    /// Window title of the active tab, including its 1-based index when there are several.
    pub fn window_title(&self) -> String {
        Self::indexed_title(
            self.visible_active_index(),
            self.visible_len(),
            &self.active().display_title(),
        )
    }
}

#[cfg(test)]
#[path = "tabs/tests.rs"]
mod tests;
