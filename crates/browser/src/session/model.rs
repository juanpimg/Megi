use super::*;

pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of tabs persisted/restored.
pub const MAX_SESSION_TABS: usize = MAX_TABS;
/// Maximum history entries stored per tab.
pub const MAX_HISTORY_PER_TAB: usize = 64;
/// File name used inside the data directory.
pub const SESSION_FILE_NAME: &str = "session.json";

/// One persisted tab.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    /// Current URL.
    pub url: String,
    /// History URLs (back/forward stack, oldest first).
    pub history: Vec<String>,
    /// Index of the current entry inside `history`.
    pub index: usize,
    /// Page zoom factor.
    pub zoom: f64,
    /// Vertical scroll offset in CSS pixels.
    pub scroll: u32,
}

/// Whole persisted session.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// Schema version of the document.
    pub version: u32,
    /// Index of the active tab.
    pub active: usize,
    /// Tabs, in order.
    pub tabs: Vec<SessionTab>,
    /// Global recent history (newest first, optional extension; empty in old files).
    pub recent: Vec<crate::history::HistoryEntry>,
    /// Per-origin zoom values (newest first, capped, optional extension).
    pub site_zooms: Vec<(String, f64)>,
    /// Reader-mode page (a `data:` URL) for the active tab, when it was in reader mode
    /// (#1725). Reader mode is a window-global view, so it is persisted at session level.
    /// Old sessions without this key parse as `None`.
    pub reader_html: Option<String>,
}

impl Session {
    /// Builds an empty session (no tabs).
    pub const fn empty() -> Self {
        Self {
            version: SCHEMA_VERSION,
            active: 0,
            tabs: Vec::new(),
            recent: Vec::new(),
            site_zooms: Vec::new(),
            reader_html: None,
        }
    }

    /// Captures the live tabs into a session document (caps history and tab counts).
    pub fn capture(tabs: &TabsState) -> Self {
        let mut session_tabs = Vec::with_capacity(tabs.len().min(MAX_SESSION_TABS));
        for tab in tabs.tabs().iter().take(MAX_SESSION_TABS) {
            let mut history: Vec<String> = tab
                .navigation
                .history()
                .iter()
                .map(|url| url.as_str())
                .collect();
            // Keep a window that contains the current entry: dropping the oldest entries
            // without remapping pointed the restored back/forward stack at the wrong URL.
            let mut index = tab.navigation.index();
            if history.len() > MAX_HISTORY_PER_TAB {
                let start = index
                    .saturating_sub(MAX_HISTORY_PER_TAB - 1)
                    .min(history.len() - MAX_HISTORY_PER_TAB);
                history.drain(..start);
                index = index.saturating_sub(start);
                history.truncate(MAX_HISTORY_PER_TAB);
            }
            let index = index.min(history.len().saturating_sub(1));
            let mut url = tab.url.clone();
            if url.is_empty()
                && let Some(current) = tab.navigation.current()
            {
                url = current.as_str();
            }
            let mut zoom = tab.zoom;
            if !zoom.is_finite() || zoom <= 0.0 {
                zoom = 1.0;
            }
            session_tabs.push(SessionTab {
                url,
                history,
                index,
                zoom,
                scroll: tab.scroll.offset(),
            });
        }
        let active = tabs
            .active_index()
            .min(session_tabs.len().saturating_sub(1));
        Self {
            version: SCHEMA_VERSION,
            active,
            tabs: session_tabs,
            recent: Vec::new(),
            site_zooms: Vec::new(),
            reader_html: None,
        }
    }

    /// Normalizes a decoded session: caps counts, clamps fields and drops unusable tabs.
    pub fn normalize(&mut self) {
        self.version = SCHEMA_VERSION;
        self.tabs.truncate(MAX_SESSION_TABS);
        for tab in &mut self.tabs {
            if !tab.zoom.is_finite() || tab.zoom <= 0.0 {
                tab.zoom = 1.0;
            }
            // Remap the index while dropping unusable URLs so it keeps pointing at the same
            // entry (retaining a url before it shifted every later index).
            let mut new_index = tab.index;
            let mut kept = 0usize;
            tab.history.retain(|url| {
                let usable = megi_webview::net::Url::parse(url).is_ok();
                if usable {
                    kept += 1;
                } else if kept <= new_index {
                    new_index = new_index.saturating_sub(1);
                }
                usable
            });
            tab.index = new_index;
            if tab.history.len() > MAX_HISTORY_PER_TAB {
                let start = tab
                    .index
                    .saturating_sub(MAX_HISTORY_PER_TAB - 1)
                    .min(tab.history.len() - MAX_HISTORY_PER_TAB);
                tab.history.drain(..start);
                tab.index = tab.index.saturating_sub(start);
                tab.history.truncate(MAX_HISTORY_PER_TAB);
            }
            tab.index = tab.index.min(tab.history.len().saturating_sub(1));
            tab.zoom = tab.zoom.clamp(0.5, 3.0);
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self.recent.truncate(crate::history::MAX_RECENT_HISTORY);
        self.site_zooms = crate::zoom::normalize_site_zooms(std::mem::take(&mut self.site_zooms));
    }

    /// Restores the session into a usable [`TabsState`] (no network is touched here).
    pub fn restore(&self) -> TabsState {
        let mut restored: Vec<Tab> = Vec::new();
        for session_tab in &self.tabs {
            let history: Vec<megi_webview::net::Url> = session_tab
                .history
                .iter()
                .filter_map(|url| megi_webview::net::Url::parse(url).ok())
                .collect();
            let url = if session_tab.url.is_empty() {
                match history.get(session_tab.index) {
                    Some(url) => url.as_str(),
                    None => continue,
                }
            } else {
                session_tab.url.clone()
            };
            let navigation = NavigationState::from_history(history, session_tab.index);
            // Preserve the scroll offset until the first frame reports the content height.
            let scroll = ScrollState::new(session_tab.scroll.saturating_add(1), 1);
            let mut scroll = scroll;
            scroll.scroll_to(session_tab.scroll);
            restored.push(Tab::restored(navigation, url, session_tab.zoom, scroll));
        }
        if restored.is_empty() {
            TabsState::new(crate::tabs::BLANK_TAB_URL)
        } else {
            TabsState::from_parts(restored, self.active)
        }
    }

    /// Serializes the session as deterministic JSON.
    pub fn to_json(&self) -> String {
        let tabs: Vec<String> = self
            .tabs
            .iter()
            .map(|tab| {
                let history: Vec<String> = tab.history.iter().map(|url| json_string(url)).collect();
                format!(
                    "{{\"url\":{},\"history\":[{}],\"index\":{},\"zoom\":{},\"scroll\":{}}}",
                    json_string(&tab.url),
                    history.join(","),
                    tab.index,
                    json_number(tab.zoom),
                    tab.scroll
                )
            })
            .collect();
        let recent: Vec<String> = self
            .recent
            .iter()
            .map(|entry| {
                format!(
                    "{{\"title\":{},\"url\":{},\"time\":{}}}",
                    json_string(&entry.title),
                    json_string(&entry.url),
                    entry.logical_time
                )
            })
            .collect();
        let zooms: Vec<String> = self
            .site_zooms
            .iter()
            .map(|(origin, zoom)| format!("[{},{}]", json_string(origin), zoom))
            .collect();
        // #1725: keep the reader-mode page so an `about:reader` tab restores its content
        // instead of an empty placeholder. Optional key: old sessions remain parseable.
        let reader_html = self
            .reader_html
            .as_deref()
            .map(|html| format!(",\"readerHtml\":{}", json_string(html)))
            .unwrap_or_default();
        format!(
            "{{\"version\":{},\"active\":{},\"tabs\":[{}],\"recent\":[{}],\"siteZooms\":[{}]{}}}",
            self.version,
            self.active,
            tabs.join(","),
            recent.join(","),
            zooms.join(","),
            reader_html
        )
    }

    /// Parses and normalizes a session JSON document.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value = JsonParser::new(text).parse()?;
        let root = value.as_object().ok_or("session root is not an object")?;
        let get = |key: &str| {
            root.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
        };
        let version = get("version")
            .and_then(JsonValue::as_u32)
            .ok_or("missing session version")?;
        if version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported session version {version} (expected {SCHEMA_VERSION})"
            ));
        }
        let active = get("active").and_then(JsonValue::as_u32).unwrap_or(0) as usize;
        let tabs_value = get("tabs").ok_or("missing session tabs")?;
        let tabs_array = tabs_value
            .as_array()
            .ok_or("session tabs is not an array")?;
        let mut tabs = Vec::new();
        for value in tabs_array.iter().take(MAX_SESSION_TABS) {
            let Some(object) = value.as_object() else {
                continue;
            };
            let tab_get = |key: &str| {
                object
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value)
            };
            let Some(url) = tab_get("url").and_then(JsonValue::as_str) else {
                continue;
            };
            let history: Vec<String> = tab_get("history")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .take(MAX_HISTORY_PER_TAB)
                        .filter_map(JsonValue::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let index = tab_get("index").and_then(JsonValue::as_u32).unwrap_or(0) as usize;
            let zoom = tab_get("zoom").and_then(JsonValue::as_f64).unwrap_or(1.0);
            let scroll = tab_get("scroll").and_then(JsonValue::as_u32).unwrap_or(0);
            tabs.push(SessionTab {
                url: url.to_owned(),
                history,
                index,
                zoom,
                scroll,
            });
        }
        let mut recent = Vec::new();
        if let Some(entries) = get("recent").and_then(JsonValue::as_array) {
            for value in entries.iter().take(crate::history::MAX_RECENT_HISTORY) {
                let Some(object) = value.as_object() else {
                    continue;
                };
                let entry_get = |key: &str| {
                    object
                        .iter()
                        .find(|(name, _)| name == key)
                        .map(|(_, value)| value)
                };
                let (Some(title), Some(url)) = (
                    entry_get("title").and_then(JsonValue::as_str),
                    entry_get("url").and_then(JsonValue::as_str),
                ) else {
                    continue;
                };
                let time = entry_get("time").and_then(JsonValue::as_u32).unwrap_or(0) as u64;
                recent.push(crate::history::HistoryEntry {
                    title: title.to_owned(),
                    url: url.to_owned(),
                    logical_time: time,
                });
            }
        }
        let mut site_zooms = Vec::new();
        if let Some(entries) = get("siteZooms").and_then(JsonValue::as_array) {
            for value in entries {
                let Some(pair) = value.as_array() else {
                    continue;
                };
                let Some(origin) = pair.first().and_then(JsonValue::as_str) else {
                    continue;
                };
                let Some(zoom) = pair.get(1).and_then(JsonValue::as_f64) else {
                    continue;
                };
                site_zooms.push((origin.to_owned(), zoom));
            }
        }
        // #1725: the reader-mode data URL, if this session was saved while in reader mode.
        let reader_html = get("readerHtml")
            .and_then(JsonValue::as_str)
            .filter(|html| html.starts_with("data:text/html,"))
            .map(str::to_owned);
        let mut session = Self {
            version: SCHEMA_VERSION,
            active,
            tabs,
            recent,
            site_zooms: crate::zoom::normalize_site_zooms(site_zooms),
            reader_html,
        };
        session.normalize();
        Ok(session)
    }
}
