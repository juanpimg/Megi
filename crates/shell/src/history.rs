//! Navigable browsing history metadata: per-tab log, global recent list, history menu state
//! and the generated `about:history` page (HTML produced by [`crate::pages`]).
//!
//! [`NavigationState`] already owns the per-tab URL stack (bounded, consecutive duplicates
//! collapsed). This module adds the **presentation metadata** the shell shows: document
//! titles, a monotonic logical timestamp (deterministic: the shell counts recorded entries,
//! no wall clock) and a bounded global recent list used by the `Ctrl+H` menu.
//!
//! Limits: history is in-memory per window (optionally persisted as the global recent list
//! in `session.json`, respecting `--no-session`); titles for restored URLs fall back to the
//! URL; the internal page is static HTML without scripting, search or grouping.

use crate::pages;

/// Maximum entries kept in one tab's history log.
pub const MAX_TAB_HISTORY: usize = 64;
/// Maximum entries kept in the global recent list / history menu.
pub const MAX_RECENT_HISTORY: usize = 10;

/// One recorded history entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Document title (fallback: URL).
    pub title: String,
    /// Absolute URL.
    pub url: String,
    /// Logical timestamp: number of entries recorded before this one.
    pub logical_time: u64,
}

/// Bounded per-tab history log with consecutive-URL deduplication.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryLog {
    entries: Vec<HistoryEntry>,
}

impl HistoryLog {
    /// Creates an empty log.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records a visit; a visit to the same URL as the last entry only refreshes its title.
    pub fn record(&mut self, title: &str, url: &str, logical_time: u64) {
        let title = if title.trim().is_empty() {
            url
        } else {
            title.trim()
        };
        if let Some(last) = self.entries.last_mut()
            && last.url == url
        {
            last.title = title.to_string();
            last.logical_time = logical_time;
            return;
        }
        self.entries.push(HistoryEntry {
            title: title.to_string(),
            url: url.to_string(),
            logical_time,
        });
        if self.entries.len() > MAX_TAB_HISTORY {
            let excess = self.entries.len() - MAX_TAB_HISTORY;
            self.entries.drain(..excess);
        }
    }

    /// Entries, oldest first.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Rebuilds a log from persisted URLs (titles fall back to the URL).
    pub fn from_urls(urls: &[String]) -> Self {
        let mut log = Self::new();
        for (index, url) in urls.iter().enumerate() {
            log.record(url, url, index as u64);
        }
        log
    }

    /// Entries, newest first.
    pub fn recent(&self) -> Vec<HistoryEntry> {
        self.entries.iter().rev().cloned().collect()
    }
}

/// Global recent-history list (newest first, bounded, consecutive duplicates collapsed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentHistory {
    entries: Vec<HistoryEntry>,
    next_time: u64,
}

impl RecentHistory {
    /// Creates an empty list.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_time: 0,
        }
    }

    /// Records a visit and returns its logical timestamp.
    pub fn record(&mut self, title: &str, url: &str) -> u64 {
        let logical_time = self.next_time;
        self.next_time = self.next_time.saturating_add(1);
        let title = if title.trim().is_empty() {
            url
        } else {
            title.trim()
        };
        if let Some(first) = self.entries.first_mut()
            && first.url == url
        {
            first.title = title.to_string();
            first.logical_time = logical_time;
            return logical_time;
        }
        self.entries.insert(
            0,
            HistoryEntry {
                title: title.to_string(),
                url: url.to_string(),
                logical_time,
            },
        );
        self.entries.truncate(MAX_RECENT_HISTORY);
        logical_time
    }

    /// Newest-first entries.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Last [`MAX_RECENT_HISTORY`] entries (menu contents).
    pub fn menu_entries(&self) -> Vec<HistoryEntry> {
        self.entries
            .iter()
            .take(MAX_RECENT_HISTORY)
            .cloned()
            .collect()
    }

    /// Rebuilds the list from persisted entries (keeps their logical times).
    pub fn from_entries(entries: Vec<HistoryEntry>) -> Self {
        let next_time = entries
            .iter()
            .map(|entry| entry.logical_time.saturating_add(1))
            .max()
            .unwrap_or(0);
        Self {
            entries: entries.into_iter().take(MAX_RECENT_HISTORY).collect(),
            next_time,
        }
    }
}

/// Open `Ctrl+H` history menu state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryMenu {
    /// Entries shown (newest first).
    pub entries: Vec<HistoryEntry>,
    /// Selected row.
    pub selected: usize,
    /// Anchor position in chrome CSS coordinates.
    pub x: f32,
    /// Anchor position in chrome CSS coordinates.
    pub y: f32,
}

impl HistoryMenu {
    /// Opens the menu with `entries` at the address-bar anchor.
    pub fn open(entries: Vec<HistoryEntry>, x: f32, y: f32) -> Self {
        Self {
            entries,
            selected: 0,
            x,
            y,
        }
    }

    /// Moves the selection up/down with wrap-around.
    pub fn move_selection(&mut self, down: bool) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        self.selected = if down {
            (self.selected + 1) % self.entries.len()
        } else if self.selected == 0 {
            self.entries.len() - 1
        } else {
            self.selected - 1
        };
        true
    }

    /// Selected entry, if any.
    pub fn selected_entry(&self) -> Option<&HistoryEntry> {
        self.entries.get(self.selected)
    }
}

/// Whether `url` is the generated internal history page (`about:history`/`megi://history`).
pub fn is_history_url(url: &str) -> bool {
    matches!(
        url.trim().to_ascii_lowercase().as_str(),
        "about:history" | "megi://history" | "megi://history/"
    )
}

/// Generates the internal history page (`about:history`) from `entries` (newest first).
pub fn history_page_html(entries: &[HistoryEntry]) -> String {
    pages::history_page(entries)
}

/// Domain shown in the history menu thumbnails (`host`, `file`, `data` or the raw URL).
pub fn domain_of(url: &str) -> String {
    let Ok(parsed) = net::Url::parse(url) else {
        return url.chars().take(24).collect();
    };
    match parsed.scheme() {
        net::Scheme::Http | net::Scheme::Https => {
            let host = parsed.host();
            if host.is_empty() {
                "local".to_string()
            } else {
                host.to_string()
            }
        }
        net::Scheme::File => "file".to_string(),
        net::Scheme::Data => "data".to_string(),
    }
}

/// Deterministic thumbnail badge for a URL: one uppercase letter plus a stable RGB color
/// derived from the domain (no images, no randomness).
pub fn domain_badge(url: &str) -> (char, [u8; 3]) {
    const PALETTE: [[u8; 3]; 8] = [
        [0x1F, 0x6F, 0xEB],
        [0x2D, 0xA4, 0x4E],
        [0xBF, 0x39, 0x83],
        [0xD2, 0x4D, 0x1F],
        [0x8B, 0x5C, 0xF6],
        [0x0F, 0x76, 0x6E],
        [0xB4, 0x79, 0x00],
        [0x55, 0x5F, 0x6B],
    ];
    let domain = domain_of(url);
    let letter = domain
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .unwrap_or('?')
        .to_ascii_uppercase();
    let mut hash = 0usize;
    for byte in domain.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as usize);
    }
    (letter, PALETTE[hash % PALETTE.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_log_dedupes_consecutive_urls_and_caps() {
        let mut log = HistoryLog::new();
        log.record("One", "https://example.com/1", 0);
        log.record("One again", "https://example.com/1", 1);
        log.record("Two", "https://example.com/2", 2);
        assert_eq!(log.len(), 2);
        assert_eq!(log.entries()[0].title, "One again");
        for index in 0..(MAX_TAB_HISTORY + 20) {
            log.record(
                &format!("Page {index}"),
                &format!("https://example.com/{index}"),
                index as u64,
            );
        }
        assert_eq!(log.len(), MAX_TAB_HISTORY);
        assert!(log.entries().last().is_some());
    }

    #[test]
    fn empty_titles_fall_back_to_the_url() {
        let mut log = HistoryLog::new();
        log.record("  ", "https://example.com/x", 0);
        assert_eq!(log.entries()[0].title, "https://example.com/x");
    }

    #[test]
    fn recent_history_is_newest_first_bounded_and_deduped() {
        let mut recent = RecentHistory::new();
        recent.record("A", "https://example.com/a");
        recent.record("B", "https://example.com/b");
        recent.record("B refreshed", "https://example.com/b");
        assert_eq!(recent.entries().len(), 2);
        assert_eq!(recent.entries()[0].url, "https://example.com/b");
        assert_eq!(recent.entries()[0].title, "B refreshed");
        for index in 0..(MAX_RECENT_HISTORY + 5) {
            recent.record(
                &format!("Entry {index}"),
                &format!("https://example.com/{index}"),
            );
        }
        assert_eq!(recent.entries().len(), MAX_RECENT_HISTORY);
        assert_eq!(recent.entries()[0].url, "https://example.com/14");
        // Round-trip via persisted entries keeps logical times.
        let restored = RecentHistory::from_entries(recent.entries().to_vec());
        assert_eq!(restored.menu_entries().len(), MAX_RECENT_HISTORY);
        assert!(restored.entries()[0].logical_time >= restored.entries()[1].logical_time);
    }

    #[test]
    fn history_url_detection() {
        assert!(is_history_url("about:history"));
        assert!(is_history_url("MEGI://history/"));
        assert!(!is_history_url("about:blank"));
        assert!(!is_history_url("https://example.com/history"));
    }

    #[test]
    fn history_menu_selection_wraps() {
        let entries: Vec<HistoryEntry> = (0..3)
            .map(|index| HistoryEntry {
                title: format!("T{index}"),
                url: format!("https://example.com/{index}"),
                logical_time: index,
            })
            .collect();
        let mut menu = HistoryMenu::open(entries, 10.0, 20.0);
        assert!(menu.move_selection(true));
        assert_eq!(menu.selected, 1);
        assert!(menu.move_selection(false));
        assert!(menu.move_selection(false));
        assert_eq!(menu.selected, 2, "wraps upward");
        assert_eq!(
            menu.selected_entry().expect("entry").url,
            "https://example.com/2"
        );
    }

    #[test]
    fn history_page_html_is_static_and_escapes() {
        let entries = vec![HistoryEntry {
            title: "A <script> & \"title\"".to_string(),
            url: "https://example.com/a?x=1&y=2".to_string(),
            logical_time: 3,
        }];
        let html = history_page_html(&entries);
        assert!(html.contains("<title>History</title>"), "{html}");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("href=\"https://example.com/a?x=1&amp;y=2\""));
        assert!(html.contains("History"));
    }
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;

    #[test]
    fn domain_badges_are_deterministic_and_cover_schemes() {
        assert_eq!(domain_of("https://Example.com/a"), "example.com");
        assert_eq!(domain_of("file:///tmp/a.html"), "file");
        assert_eq!(domain_of("data:text/html,x"), "data");
        let first = domain_badge("https://example.com/page");
        let second = domain_badge("https://example.com/other");
        assert_eq!(first, second, "same domain ⇒ same badge");
        assert_eq!(first.0, 'E');
        assert!(domain_badge("not a url").0.is_ascii_uppercase());
    }
}
