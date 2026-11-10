//! Browser history: a pure, display-free navigation state machine.
//!
//! The shell keeps one [`NavigationState`] per window. Every committed navigation
//! (initial load, link click, `back`/`forward`, reload-target change) is a normal
//! [`NavigationState::navigate_to`]; render redirects replace the current entry
//! ([`NavigationState::replace_current`]) instead of pushing a new one.
//!
//! ## Semantics (mirroring the HTML session history model)
//!
//! - Navigating while the user is in the past (after one or more `back`) **truncates the
//!   forward entries** before pushing the new URL.
//! - Navigating to the URL that is already current is a no-op
//!   ([`NavigationOutcome::SameUrl`]) so clicking the current page's self-link or
//!   pressing reload does not grow the history.
//! - The history is bounded by [`MAX_HISTORY`] entries; the oldest entries are dropped
//!   first and the current index is adjusted accordingly.
//! - `back`/`forward` never cross the ends (`None` and no state change).
//! - Failed navigations keep their history entry (the URL *was* visited); the shell shows
//!   an error frame, and `back` still works.
//!
//! The state stores [`net::Url`] values only; it has no rendering, no timers and no I/O,
//! so it is fully unit-tested without a display server.

use net::Url;

/// Maximum number of entries kept in a window's history (oldest are dropped first).
pub const MAX_HISTORY: usize = 256;

/// Result of a [`NavigationState::navigate_to`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationOutcome {
    /// A new entry was appended (forward entries were discarded when navigating from
    /// the past).
    Pushed,
    /// The target URL is already the current one; the state did not change.
    SameUrl,
}

/// Session history of one window: a list of visited URLs plus the current position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationState {
    history: Vec<Url>,
    index: usize,
}

impl NavigationState {
    /// Creates an empty history (no current URL).
    pub const fn new() -> Self {
        Self {
            history: Vec::new(),
            index: 0,
        }
    }

    /// Returns the current URL, if any navigation happened yet.
    pub fn current(&self) -> Option<&Url> {
        self.history.get(self.index)
    }

    /// Returns the current URL as its serialized string, if any.
    pub fn current_url(&self) -> Option<String> {
        self.current().map(Url::as_str)
    }

    /// Returns the number of stored entries.
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Returns `true` when no navigation happened yet.
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    /// Returns the index of the current entry (0 before the first navigation).
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns every stored entry in visit order.
    pub fn history(&self) -> &[Url] {
        &self.history
    }

    /// Returns `true` when there is an older entry to go back to.
    pub const fn can_go_back(&self) -> bool {
        self.index > 0
    }

    /// Returns `true` when there is a newer entry to go forward to.
    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }

    /// Commits a navigation to `url`, truncating forward entries when navigating from the
    /// past and appending a new entry. A navigation to the current URL is a no-op.
    pub fn navigate_to(&mut self, url: Url) -> NavigationOutcome {
        if self.current() == Some(&url) {
            return NavigationOutcome::SameUrl;
        }
        // Navigating from the past discards the forward entries (HTML session history).
        self.history.truncate(self.index + 1);
        self.history.push(url);
        if self.history.len() > MAX_HISTORY {
            let excess = self.history.len() - MAX_HISTORY;
            self.history.drain(..excess);
        }
        self.index = self.history.len() - 1;
        NavigationOutcome::Pushed
    }

    /// Replaces the current entry (used for server redirects of the requested URL).
    ///
    /// Returns `false` when the history is empty (nothing to replace) or when the URL is
    /// unchanged.
    pub fn replace_current(&mut self, url: Url) -> bool {
        match self.history.get_mut(self.index) {
            Some(entry) if *entry != url => {
                *entry = url;
                true
            }
            _ => false,
        }
    }

    /// Moves back one entry and returns the new current URL.
    pub fn back(&mut self) -> Option<&Url> {
        if !self.can_go_back() {
            return None;
        }
        self.index -= 1;
        self.current()
    }

    /// Moves forward one entry and returns the new current URL.
    pub fn forward(&mut self) -> Option<&Url> {
        if !self.can_go_forward() {
            return None;
        }
        self.index += 1;
        self.current()
    }

    /// First (home) history entry — the destination of `Alt+Home`.
    pub fn home_target(&self) -> Option<&Url> {
        self.history.first()
    }

    /// Returns the URL to reload (the current one).
    pub fn reload_target(&self) -> Option<&Url> {
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).expect("test URL parses")
    }

    #[test]
    fn empty_state_has_no_current_and_cannot_navigate() {
        let mut state = NavigationState::new();
        assert!(state.current().is_none());
        assert!(state.is_empty());
        assert!(!state.can_go_back());
        assert!(!state.can_go_forward());
        assert_eq!(state.back(), None);
        assert_eq!(state.forward(), None);
        assert_eq!(state.reload_target(), None);
        assert!(!state.replace_current(url("https://example.com/")));
    }

    #[test]
    fn first_navigation_pushes_and_becomes_current() {
        let mut state = NavigationState::new();
        assert_eq!(
            state.navigate_to(url("https://example.com/a")),
            NavigationOutcome::Pushed
        );
        assert_eq!(state.len(), 1);
        assert_eq!(
            state.current_url().as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(state.index(), 0);
        assert!(!state.can_go_back());
        assert!(!state.can_go_forward());
    }

    #[test]
    fn navigations_append_and_back_forward_walk_the_history() {
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/a"));
        state.navigate_to(url("https://example.com/b"));
        state.navigate_to(url("https://example.com/c"));
        assert_eq!(state.len(), 3);
        assert!(state.can_go_back());
        assert!(!state.can_go_forward());

        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("https://example.com/b")
        );
        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(state.back(), None, "cannot go before the first entry");
        assert_eq!(
            state.forward().map(Url::as_str).as_deref(),
            Some("https://example.com/b")
        );
        assert_eq!(
            state.forward().map(Url::as_str).as_deref(),
            Some("https://example.com/c")
        );
        assert_eq!(state.forward(), None, "cannot go past the last entry");
    }

    #[test]
    fn navigations_from_the_past_discard_forward_entries() {
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/a"));
        state.navigate_to(url("https://example.com/b"));
        state.navigate_to(url("https://example.com/c"));
        state.back();
        state.back();
        assert!(state.can_go_forward());

        assert_eq!(
            state.navigate_to(url("https://example.com/d")),
            NavigationOutcome::Pushed
        );
        assert_eq!(state.len(), 2, "forward entries must be discarded");
        assert_eq!(
            state.current_url().as_deref(),
            Some("https://example.com/d")
        );
        assert!(!state.can_go_forward());
        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn navigating_to_the_current_url_is_a_noop() {
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/a"));
        assert_eq!(
            state.navigate_to(url("https://example.com/a")),
            NavigationOutcome::SameUrl
        );
        assert_eq!(state.len(), 1);
        assert_eq!(state.index(), 0);
    }

    #[test]
    fn replace_current_updates_only_the_current_entry() {
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/a"));
        state.navigate_to(url("https://example.com/b"));
        assert!(state.replace_current(url("https://example.com/redirected")));
        assert_eq!(state.len(), 2);
        assert_eq!(
            state.current_url().as_deref(),
            Some("https://example.com/redirected")
        );
        assert!(!state.replace_current(url("https://example.com/redirected")));
        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn history_is_bounded_and_drops_oldest_entries() {
        let mut state = NavigationState::new();
        for index in 0..(MAX_HISTORY + 10) {
            state.navigate_to(url(&format!("https://example.com/{index}")));
        }
        assert_eq!(state.len(), MAX_HISTORY);
        assert_eq!(
            state.current_url().as_deref(),
            Some(format!("https://example.com/{}", MAX_HISTORY + 9).as_str())
        );
        // The oldest 10 entries were dropped: going all the way back reaches entry 10.
        while state.can_go_back() {
            state.back();
        }
        assert_eq!(
            state.current_url().as_deref(),
            Some("https://example.com/10")
        );
    }

    #[test]
    fn failed_navigation_keeps_its_entry_and_back_still_works() {
        // A failed fetch is modelled by pushing the URL and then not receiving a frame:
        // history and `back` must be unaffected by the render error.
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/ok"));
        state.navigate_to(url("https://invalid.example/fails"));
        assert_eq!(state.len(), 2);
        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("https://example.com/ok")
        );
        assert!(state.can_go_forward());
        assert_eq!(
            state.forward().map(Url::as_str).as_deref(),
            Some("https://invalid.example/fails")
        );
    }

    #[test]
    fn file_and_data_urls_are_tracked_like_any_other() {
        let mut state = NavigationState::new();
        state.navigate_to(url("file:///tmp/page.html"));
        state.navigate_to(url("data:text/html,hello"));
        assert_eq!(state.len(), 2);
        assert_eq!(
            state.back().map(Url::as_str).as_deref(),
            Some("file:///tmp/page.html")
        );
    }

    #[test]
    fn reload_target_is_the_current_entry() {
        let mut state = NavigationState::new();
        state.navigate_to(url("https://example.com/a"));
        state.navigate_to(url("https://example.com/b"));
        state.back();
        assert_eq!(
            state.reload_target().map(Url::as_str).as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn home_target_is_the_first_history_entry() {
        let mut state = NavigationState::new();
        assert!(state.home_target().is_none());
        state.navigate_to(url("file:///home/index.html"));
        state.navigate_to(url("https://example.com/a"));
        assert_eq!(
            state.home_target().map(Url::as_str),
            Some("file:///home/index.html".to_string())
        );
        assert_eq!(
            state.current_url().as_deref(),
            Some("https://example.com/a")
        );
    }
}
