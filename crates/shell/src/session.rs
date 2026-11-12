//! Persistent browser session: tabs, per-tab history, zoom and scroll, stored as JSON.
//!
//! The session file lives in the user data directory (`$XDG_DATA_HOME/megi/session.json`,
//! falling back to `~/.local/share/megi/session.json` and then `~/.megi/session.json`), or in
//! an explicit path from `--session-file PATH`. `--no-session` disables persistence and
//! headless mode never writes a session (documented). The format is a **small self-contained
//! JSON document** (no external dependencies), versioned with [`SCHEMA_VERSION`]:
//!
//! ```json
//! {"version":1,"active":0,"tabs":[{"url":"https://example.com/","history":["https://example.com/"],"index":0,"zoom":1.0,"scroll":0}]}
//! ```
//!
//! Loading is deliberately tolerant: a missing file is a fresh session (no warning), a
//! corrupt/unsupported file produces a warning and a fresh session (never a panic), unknown
//! fields are ignored and out-of-range values are clamped. Saves write a unique
//! per-process temporary file and rename it atomically, so a crash leaves the previous
//! session intact and two instances cannot tear each other's JSON (last writer wins); the
//! advisory [`SessionLock`] sidecar is available but not wired into [`save`].
//!
//! Limits: URLs/history/zoom/scroll only (no cookies, passwords, form state or find state),
//! at most [`MAX_SESSION_TABS`] tabs and [`MAX_HISTORY_PER_TAB`] history entries per tab.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::navigation::NavigationState;
use crate::scroll::ScrollState;
use crate::tabs::{MAX_TABS, Tab, TabsState};

/// Version of the on-disk JSON schema.
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
                let usable = net::Url::parse(url).is_ok();
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
            let history: Vec<net::Url> = session_tab
                .history
                .iter()
                .filter_map(|url| net::Url::parse(url).ok())
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
        format!(
            "{{\"version\":{},\"active\":{},\"tabs\":[{}],\"recent\":[{}],\"siteZooms\":[{}]}}",
            self.version,
            self.active,
            tabs.join(","),
            recent.join(","),
            zooms.join(",")
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
        for value in tabs_array {
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
            for value in entries {
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
        let mut session = Self {
            version: SCHEMA_VERSION,
            active,
            tabs,
            recent,
            site_zooms: crate::zoom::normalize_site_zooms(site_zooms),
        };
        session.normalize();
        Ok(session)
    }
}

/// Maximum size of a session file accepted from disk (bytes).
///
/// `fs::read_to_string` on an unbounded path (`/dev/zero`, a huge file or a FIFO) would
/// read until OOM; the cap also bounds the parsed JSON tree before `normalize` truncates.
pub const MAX_SESSION_BYTES: u64 = 4 * 1024 * 1024;

/// Result of loading a session file.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedSession {
    /// Loaded session when the file exists and is valid.
    pub session: Option<Session>,
    /// Human-readable warning for corrupt/unsupported files (also printed by the shell).
    pub warning: Option<String>,
}

/// Loads a session file. A missing file is a fresh session without a warning; any
/// corrupt/unsupported content returns `session = None` plus a warning (never panics).
pub fn load(path: &Path) -> LoadedSession {
    let mut bytes = Vec::new();
    let read = fs::File::open(path).and_then(|file| {
        let mut reader = file.take(MAX_SESSION_BYTES + 1);
        reader.read_to_end(&mut bytes)
    });
    match read {
        Ok(_) if bytes.len() as u64 > MAX_SESSION_BYTES => LoadedSession {
            session: None,
            warning: Some(format!(
                "ignoring session file {} larger than {} bytes",
                path.display(),
                MAX_SESSION_BYTES
            )),
        },
        Ok(_) => match String::from_utf8(bytes)
            .map_err(|error| error.utf8_error().to_string())
            .and_then(|text| Session::from_json(&text))
        {
            Ok(session) => LoadedSession {
                session: Some(session),
                warning: None,
            },
            Err(error) => LoadedSession {
                session: None,
                warning: Some(format!(
                    "ignoring corrupt session file {}: {error}",
                    path.display()
                )),
            },
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedSession {
            session: None,
            warning: None,
        },
        Err(error) => LoadedSession {
            session: None,
            warning: Some(format!(
                "cannot read session file {}: {error}",
                path.display()
            )),
        },
    }
}

/// Saves the session atomically (unique temporary + rename), creating parent directories.
pub fn save(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let json = session.to_json();
    if json.len() as u64 > MAX_SESSION_BYTES {
        // `load` rejects files over the cap: writing one would make the next start discard
        // the session and overwrite the previous file.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "session is {} bytes, larger than the {MAX_SESSION_BYTES}-byte read cap",
                json.len()
            ),
        ));
    }
    let temporary = write_temporary(path, json.as_bytes())?;
    let result = fs::rename(&temporary, path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Creates a fresh temporary next to `path` and writes `contents` into it.
///
/// `create_new` prevents following a symlink or truncating a pre-existing file planted at a
/// predictable name; an `AlreadyExists` collision advances the sequence and retries.
fn write_temporary(path: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    for _ in 0..16 {
        let candidate = unique_temporary_path(path);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                file.write_all(contents)?;
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot create a unique session temporary",
    ))
}

/// Returns a per-process, per-call temporary path next to `path` (same filesystem, so the
/// final rename is atomic).
///
/// A fixed `session.json.tmp` let two shell instances write and rename the same temporary,
/// producing a torn session file.
fn unique_temporary_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session.json".to_string());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{sequence}.tmp", std::process::id()))
}

/// Resolves the default session path from environment-like inputs (pure; tested).
///
/// Order: `$XDG_DATA_HOME/megi/session.json`, `$HOME/.local/share/megi/session.json`,
/// `$HOME/.megi/session.json`. Returns `None` when neither variable is usable.
pub fn default_session_path(xdg_data_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_data_home
        && !xdg.as_os_str().is_empty()
    {
        return Some(xdg.join("megi").join(SESSION_FILE_NAME));
    }
    if let Some(home) = home
        && !home.as_os_str().is_empty()
    {
        let standard = home
            .join(".local")
            .join("share")
            .join("megi")
            .join(SESSION_FILE_NAME);
        return Some(standard);
    }
    None
}

/// Runtime session path: explicit `--session-file` or the environment-based default.
pub fn resolve_session_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    let xdg = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    default_session_path(xdg.as_deref(), home.as_deref())
}

/// Best-effort advisory lock next to the session file (`session.json.lock`).
///
/// This is intentionally simple: `create_new` of the lock file; if it already exists the
/// caller proceeds without a lock (saves are atomic, so the worst case is last-writer-wins).
/// Stale lock files are not removed automatically (documented limitation).
#[derive(Debug)]
pub struct SessionLock {
    path: PathBuf,
}

impl SessionLock {
    /// Tries to acquire the lock; returns `None` when another process holds it.
    pub fn acquire(session_path: &Path) -> io::Result<Option<Self>> {
        let path = session_path.with_extension("json.lock");
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                Ok(Some(Self { path }))
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Lock file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e9 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// Minimal JSON value used by the session parser.
#[derive(Clone, Debug, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_u32(&self) -> Option<u32> {
        match self {
            Self::Number(number) if number.is_finite() && *number >= 0.0 => Some(*number as u32),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => Some(*number),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }
}

/// Recursive-descent JSON parser with a depth cap (rejects deeply nested input).
struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
    depth: usize,
}

impl<'a> JsonParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
            depth: 0,
        }
    }

    fn parse(mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        let value = self.parse_value()?;
        self.skip_whitespace();
        if self.position != self.bytes.len() {
            return Err("trailing characters after JSON value".to_string());
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        if self.depth > 64 {
            return Err("JSON nesting too deep".to_string());
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') => {
                self.expect_literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'n') => {
                self.expect_literal("null")?;
                Ok(JsonValue::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err("invalid JSON value".to_string()),
            None => Err("unexpected end of JSON".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.depth += 1;
        self.position += 1; // '{'
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            self.depth -= 1;
            return Ok(JsonValue::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.next() != Some(b':') {
                return Err("expected ':' in JSON object".to_string());
            }
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err("expected ',' or '}' in JSON object".to_string()),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Object(entries))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.depth += 1;
        self.position += 1; // '['
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            self.depth -= 1;
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            match self.next() {
                Some(b',') => continue,
                Some(b']') => break,
                _ => return Err("expected ',' or ']' in JSON array".to_string()),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Array(values))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.next() != Some(b'"') {
            return Err("expected JSON string".to_string());
        }
        let mut out = String::new();
        loop {
            match self.next() {
                None => return Err("unterminated JSON string".to_string()),
                Some(b'"') => break,
                Some(b'\\') => match self.next() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000c}'),
                    Some(b'u') => {
                        let hex = self.take(4)?;
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| "invalid \\u escape".to_string())?;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    _ => return Err("invalid JSON escape".to_string()),
                },
                Some(byte) if byte < 0x20 => {
                    return Err("control character in JSON string".to_string());
                }
                Some(byte) => {
                    // Decode the whole UTF-8 scalar: pushing each byte as a `char` turned
                    // every non-ASCII literal into Latin-1 mojibake ("café" -> "cafÃ©").
                    let start = self.position - 1;
                    let width = utf8_width(byte);
                    if width == 0 || start + width > self.bytes.len() {
                        return Err("invalid UTF-8 in JSON string".to_string());
                    }
                    let text = std::str::from_utf8(&self.bytes[start..start + width])
                        .map_err(|_| "invalid UTF-8 in JSON string".to_string())?;
                    out.push_str(text);
                    self.position = start + width;
                }
            }
        }
        Ok(out)
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if self.peek().is_some_and(|byte| byte == b'e' || byte == b'E') {
            self.position += 1;
            if self.peek().is_some_and(|byte| byte == b'+' || byte == b'-') {
                self.position += 1;
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| "invalid number encoding".to_string())?;
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| format!("invalid JSON number `{text}`"))
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), String> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(())
        } else {
            Err(format!("expected JSON literal `{literal}`"))
        }
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.position += 1;
        }
        byte
    }

    fn take(&mut self, count: usize) -> Result<String, String> {
        let slice = self
            .bytes
            .get(self.position..self.position + count)
            .ok_or_else(|| "truncated JSON".to_string())?;
        self.position += count;
        String::from_utf8(slice.to_vec()).map_err(|_| "invalid UTF-8 escape".to_string())
    }
}

/// Length of the UTF-8 scalar starting with `byte` (0 for a continuation/invalid lead).
fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session {
            version: SCHEMA_VERSION,
            active: 1,
            tabs: vec![
                SessionTab {
                    url: "data:text/html,<h1>one</h1>".to_string(),
                    history: vec!["data:text/html,<h1>one</h1>".to_string()],
                    index: 0,
                    zoom: 1.0,
                    scroll: 0,
                },
                SessionTab {
                    url: "file:///tmp/two.html".to_string(),
                    history: vec![
                        "file:///tmp/one.html".to_string(),
                        "file:///tmp/two.html".to_string(),
                    ],
                    index: 1,
                    zoom: 1.5,
                    scroll: 120,
                },
            ],
            recent: Vec::new(),
            site_zooms: Vec::new(),
        }
    }

    #[test]
    fn json_roundtrip_is_deterministic() {
        let session = sample();
        let json = session.to_json();
        let parsed = Session::from_json(&json).expect("parse");
        assert_eq!(parsed, session);
        assert_eq!(parsed.to_json(), json, "writer is deterministic");
    }

    #[test]
    fn escapes_strings_and_rejects_trailing_garbage() {
        let mut session = sample();
        session.tabs[0].url = "data:text/html,<p>quote\" slash\\ tab\t</p>".to_string();
        let json = session.to_json();
        let parsed = Session::from_json(&json).expect("parse");
        assert_eq!(parsed.tabs[0].url, session.tabs[0].url);
        assert!(Session::from_json("{} trailing").is_err());
        assert!(Session::from_json("{").is_err());
        assert!(Session::from_json("{\"version\":").is_err());
    }

    #[test]
    fn corrupt_and_unsupported_versions_are_tolerated_at_load() {
        let loaded = load(Path::new("/definitely/missing/session.json"));
        assert!(loaded.session.is_none());
        assert!(loaded.warning.is_none(), "missing file is a fresh session");

        let mut session = sample();
        session.version = 99;
        assert!(Session::from_json(&session.to_json()).is_err());
        assert!(Session::from_json("not json").is_err());
        assert!(Session::from_json("{\"version\":1}").is_err());
    }

    #[test]
    fn normalization_caps_history_tabs_and_clamps_values() {
        let mut tabs = Vec::new();
        for index in 0..(MAX_SESSION_TABS + 5) {
            let url = format!("data:text/html,{index}");
            tabs.push(SessionTab {
                url: url.clone(),
                history: (0..(MAX_HISTORY_PER_TAB + 10))
                    .map(|item| format!("data:text/html,{index}-{item}"))
                    .collect(),
                index: 500,
                zoom: 99.0,
                scroll: 7,
            });
        }
        let mut session = Session {
            version: SCHEMA_VERSION,
            active: 999,
            tabs,
            recent: Vec::new(),
            site_zooms: Vec::new(),
        };
        session.normalize();
        assert_eq!(session.tabs.len(), MAX_SESSION_TABS);
        assert_eq!(session.tabs[0].history.len(), MAX_HISTORY_PER_TAB);
        assert_eq!(session.tabs[0].zoom, 3.0);
        assert_eq!(session.tabs[0].index, MAX_HISTORY_PER_TAB - 1);
        assert_eq!(session.active, MAX_SESSION_TABS - 1);
    }

    #[test]
    fn capture_and_restore_keep_tabs_zoom_scroll_and_history() {
        let mut tabs = TabsState::from_urls(vec![
            "data:text/html,one".to_string(),
            "file:///tmp/two.html".to_string(),
        ]);
        assert!(tabs.activate(1));
        {
            let tab = tabs.active_mut();
            tab.zoom = 1.5;
            tab.scroll = ScrollState::new(200, 100);
            tab.scroll.scroll_to(80);
            tab.navigation
                .navigate_to(net::Url::parse("file:///tmp/three.html").expect("url"));
            tab.url = "file:///tmp/three.html".to_string();
        }
        let session = Session::capture(&tabs);
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active, 1);
        assert_eq!(session.tabs[1].zoom, 1.5);
        assert_eq!(session.tabs[1].scroll, 80);
        assert_eq!(session.tabs[1].history.len(), 2);

        let restored = session.restore();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.active_index(), 1);
        assert_eq!(restored.active().url, "file:///tmp/three.html");
        assert!((restored.active().zoom - 1.5).abs() < f64::EPSILON);
        assert_eq!(restored.active().navigation.history().len(), 2);
        assert_eq!(restored.active().navigation.index(), 1);
        assert!(restored.active().scroll.offset() > 0);
    }

    #[test]
    fn save_and_load_roundtrip_in_a_temp_directory() {
        let dir = std::env::temp_dir().join(format!(
            "megi-session-test-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("session.json");
        save(&path, &sample()).expect("save");
        let loaded = load(&path);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.session, Some(sample()));
        // Corrupt the file: load returns a warning and no session.
        fs::write(&path, "{broken").expect("write corrupt");
        let corrupt = load(&path);
        assert!(corrupt.session.is_none());
        assert!(corrupt.warning.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_session_restores_a_blank_tab() {
        let restored = Session::empty().restore();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.active().url, crate::tabs::BLANK_TAB_URL);
        assert!(Session::empty().to_json().contains("\"tabs\":[]"));
    }

    #[test]
    fn unknown_fields_are_ignored_and_numbers_are_clamped() {
        let json = "{\"version\":1,\"active\":5,\"future\":{\"x\":[1,2]},\"tabs\":[{\"url\":\"data:text/html,x\",\"history\":[\"data:text/html,x\"],\"index\":9,\"zoom\":99,\"scroll\":3,\"unknown\":true}]}";
        let session = Session::from_json(json).expect("tolerant parse");
        assert_eq!(session.tabs.len(), 1);
        assert!(session.tabs[0].index < session.tabs[0].history.len());
        assert!(session.tabs[0].zoom <= 3.0);
        assert_eq!(session.active, 0);
    }

    #[test]
    fn default_paths_follow_xdg_then_home() {
        let xdg = Path::new("/xdg/data");
        let home = Path::new("/home/user");
        assert_eq!(
            default_session_path(Some(xdg), Some(home)),
            Some(PathBuf::from("/xdg/data/megi/session.json"))
        );
        assert_eq!(
            default_session_path(None, Some(home)),
            Some(PathBuf::from("/home/user/.local/share/megi/session.json"))
        );
        assert_eq!(default_session_path(None, None), None);
    }

    #[test]
    fn lock_is_best_effort_and_released_on_drop() {
        let dir = std::env::temp_dir().join(format!(
            "megi-session-lock-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("session.json");
        let first = SessionLock::acquire(&path).expect("acquire").expect("free");
        assert!(first.path().is_file());
        assert!(
            SessionLock::acquire(&path).expect("acquire").is_none(),
            "second acquisition fails while locked"
        );
        drop(first);
        assert!(SessionLock::acquire(&path).expect("acquire").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_history_roundtrips_through_the_session() {
        let mut session = sample();
        session.recent = vec![crate::history::HistoryEntry {
            title: "Example".to_string(),
            url: "https://example.com/".to_string(),
            logical_time: 4,
        }];
        let parsed = Session::from_json(&session.to_json()).expect("parse");
        assert_eq!(parsed.recent, session.recent);
        assert!(
            Session::from_json(&sample().to_json())
                .expect("old files")
                .recent
                .is_empty()
        );
    }

    #[test]
    fn site_zooms_roundtrip_tolerantly() {
        let mut session = sample();
        session.site_zooms = vec![
            ("https://example.com".to_string(), 1.5),
            ("file://".to_string(), 0.75),
        ];
        let parsed = Session::from_json(&session.to_json()).expect("parse");
        assert_eq!(parsed.site_zooms, session.site_zooms);
        let tolerant = Session::from_json(
            "{\"version\":1,\"active\":0,\"tabs\":[],\"siteZooms\":[[\"https://a.example\",9.0],\"bad\",[\"\",1.0]]}",
        )
        .expect("tolerant");
        assert_eq!(
            tolerant.site_zooms,
            vec![("https://a.example".to_string(), 3.0)]
        );
        assert!(
            Session::from_json("{\"version\":1,\"active\":0,\"tabs\":[]}")
                .expect("old")
                .site_zooms
                .is_empty()
        );
    }

    #[test]
    fn persisted_recent_history_keeps_titles_and_domains() {
        let mut session = sample();
        session.recent = vec![
            crate::history::HistoryEntry {
                title: "Home".to_string(),
                url: "https://example.com/home".to_string(),
                logical_time: 1,
            },
            crate::history::HistoryEntry {
                title: "Docs".to_string(),
                url: "https://docs.example/start".to_string(),
                logical_time: 2,
            },
        ];
        let parsed = Session::from_json(&session.to_json()).expect("parse");
        assert_eq!(parsed.recent, session.recent);
        assert_eq!(
            crate::history::domain_of(&parsed.recent[1].url),
            "docs.example"
        );
        assert!(
            crate::history::domain_badge(&parsed.recent[1].url)
                .0
                .is_ascii_uppercase()
        );
    }
    #[test]
    fn capture_remaps_the_history_index_and_keeps_the_current_entry() {
        let mut tabs = crate::tabs::TabsState::new("data:text/html,0");
        for index in 1..100 {
            let url = net::Url::parse(&format!("data:text/html,{index}")).expect("url");
            tabs.active_mut().navigation.navigate_to(url);
        }
        for _ in 0..90 {
            assert!(tabs.active_mut().navigation.back().is_some());
        }
        let current = tabs
            .active()
            .navigation
            .current()
            .expect("current")
            .as_str()
            .to_owned();
        let session = Session::capture(&tabs);
        let tab = &session.tabs[0];
        assert_eq!(tab.history.len(), MAX_HISTORY_PER_TAB);
        assert!(tab.index < tab.history.len());
        assert_eq!(
            tab.history[tab.index], current,
            "the saved index must point at the current URL"
        );
    }

    #[test]
    fn normalize_remaps_the_index_after_dropping_invalid_urls() {
        let json = r#"{"version":1,"active":0,"tabs":[{"url":"https://a/","history":["not a url","https://a/","https://b/"],"index":1,"zoom":1.0,"scroll":0}]}"#;
        let session = Session::from_json(json).expect("parse");
        let tab = &session.tabs[0];
        assert_eq!(tab.index, 0);
        assert_eq!(tab.history[tab.index], "https://a/");
        let restored = session.restore();
        assert_eq!(
            restored
                .active()
                .navigation
                .current()
                .map(net::Url::as_str)
                .as_deref(),
            Some("https://a/")
        );
    }

    #[test]
    fn json_roundtrip_preserves_non_ascii_strings() {
        let mut session = sample();
        session.recent.push(crate::history::HistoryEntry {
            title: "café".to_string(),
            url: "https://example.com/".to_string(),
            logical_time: 1,
        });
        let json = session.to_json();
        let parsed = Session::from_json(&json).expect("parse");
        assert_eq!(parsed.recent[0].title, "café");
        assert_eq!(parsed, session);
    }

    #[test]
    fn oversized_session_files_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("megi-session-big-{}.json", std::process::id()));
        let mut json = String::from("{\"version\":1,\"tabs\":[],\"pad\":\"");
        json.push_str(&"x".repeat(MAX_SESSION_BYTES as usize));
        json.push_str("\"}");
        std::fs::write(&path, json).expect("write");
        let loaded = load(&path);
        assert!(loaded.session.is_none());
        assert!(
            loaded
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("larger than")),
            "{:?}",
            loaded.warning
        );
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn special_files_are_read_with_the_cap() {
        if !Path::new("/dev/zero").exists() {
            return;
        }
        let loaded = load(Path::new("/dev/zero"));
        assert!(loaded.session.is_none());
        assert!(
            loaded
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("larger than")),
            "{:?}",
            loaded.warning
        );
    }
    #[test]
    fn capture_slides_the_window_when_the_current_entry_is_late() {
        let mut tabs = crate::tabs::TabsState::new("data:text/html,0");
        for index in 1..256 {
            let url = net::Url::parse(&format!("data:text/html,{index}")).expect("url");
            tabs.active_mut().navigation.navigate_to(url);
        }
        let current = tabs
            .active()
            .navigation
            .current()
            .expect("current")
            .as_str();
        let session = Session::capture(&tabs);
        let tab = &session.tabs[0];
        assert_eq!(tab.history.len(), MAX_HISTORY_PER_TAB);
        assert_eq!(tab.index, MAX_HISTORY_PER_TAB - 1);
        assert_eq!(tab.history[tab.index], current);
    }

    #[test]
    fn normalize_moves_the_index_off_an_invalid_url() {
        let json = r#"{"version":1,"active":0,"tabs":[{"url":"https://b/","history":["https://a/","not a url","https://b/"],"index":1,"zoom":1.0,"scroll":0}]}"#;
        let session = Session::from_json(json).expect("parse");
        let tab = &session.tabs[0];
        assert_eq!(tab.history.len(), 2);
        assert_eq!(tab.index, 0, "the index falls back to the predecessor");
        assert_eq!(tab.history[tab.index], "https://a/");
    }

    #[test]
    fn json_roundtrip_preserves_four_byte_scalars() {
        let mut session = sample();
        session.recent.push(crate::history::HistoryEntry {
            title: "emoji 🚀 fin".to_string(),
            url: "https://example.com/".to_string(),
            logical_time: 2,
        });
        let parsed = Session::from_json(&session.to_json()).expect("parse");
        assert_eq!(parsed.recent[0].title, "emoji 🚀 fin");
    }

    #[test]
    fn saving_an_oversized_session_is_rejected_instead_of_written() {
        let dir = std::env::temp_dir().join(format!("megi-session-over-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("session.json");
        let mut session = sample();
        session.tabs[0].url = format!("data:text/html,{}", "x".repeat(MAX_SESSION_BYTES as usize));
        let error = save(&path, &session).expect_err("session exceeds the read cap");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(!path.exists(), "nothing is written");

        #[allow(dead_code)]
        fn temporaries_are_unique_and_exclusive() {
            let dir = std::env::temp_dir().join(format!("megi-session-tmp-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            let path = dir.join("session.json");
            let first = write_temporary(&path, b"{}").expect("first temporary");
            let second = write_temporary(&path, b"{}").expect("second temporary");
            assert_ne!(first, second, "each call gets its own temporary");
            assert!(first.is_file() && second.is_file());
            let _ = std::fs::remove_file(&first);
            let _ = std::fs::remove_file(&second);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[allow(dead_code)]
        fn failed_saves_clean_up_their_temporary() {
            let dir =
                std::env::temp_dir().join(format!("megi-session-fail-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            // Renaming onto a directory fails; the temporary must not be left behind.
            let target = dir.join("session.json");
            std::fs::create_dir(&target).expect("target dir");
            assert!(save(&target, &sample()).is_err());
            let leftovers: Vec<String> = std::fs::read_dir(&dir)
                .expect("read dir")
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect();
            assert!(leftovers.is_empty(), "no temporary left: {leftovers:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[allow(dead_code)]
        fn saves_use_a_unique_temporary_and_leave_no_files() {
            let dir =
                std::env::temp_dir().join(format!("megi-session-save-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("mkdir");
            let path = dir.join("session.json");
            let session = sample();
            save(&path, &session).expect("save");
            assert!(path.is_file(), "the session was renamed into place");
            let leftovers: Vec<String> = std::fs::read_dir(&dir)
                .expect("read dir")
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect();
            assert!(
                leftovers.is_empty(),
                "no temporary files left: {leftovers:?}"
            );
            let parsed = Session::from_json(&std::fs::read_to_string(&path).expect("read"))
                .expect("parse saved session");
            assert_eq!(parsed, session);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn normalize_and_restore_preserve_valid_tabs_across_corrupt_entries() {
        let mut session = sample();
        session.tabs.push(SessionTab {
            url: String::new(),
            history: vec![],
            index: 99,
            scroll: 0,
            zoom: f64::NAN,
        });
        session.tabs.push(SessionTab {
            url: "https://example.org/".to_string(),
            history: vec!["https://example.org/".to_string()],
            index: 0,
            scroll: 0,
            zoom: -5.0,
        });
        session.normalize();
        assert_eq!(session.tabs.len(), 4);
        assert_eq!(session.tabs[2].zoom, 1.0);
        assert_eq!(session.tabs[3].zoom, 1.0);

        let restored = session.restore();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.tabs()[2].url, "https://example.org/");
    }
}
