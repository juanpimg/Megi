use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleEntry {
    /// `log`, `info`, `warn` or `error`.
    pub level: String,
    /// Space-joined stringification of the arguments.
    pub message: String,
}

/// Identity of an event target in the realm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TargetKey {
    Node(u32),
    Window,
    Document,
}

/// A registered event listener (its callback lives in the JS-side rooted table).
#[derive(Clone, Debug)]
pub(crate) struct Listener {
    pub target: TargetKey,
    pub event_type: String,
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
    pub slot: u32,
    /// Id of the `AbortSignal` from `addEventListener(..., { signal })`, if any.
    pub signal_id: Option<u32>,
}

/// An inline event handler property (`el.onclick = fn`) registered in the host.
#[derive(Clone, Debug)]
pub(crate) struct InlineHandler {
    pub target: TargetKey,
    pub event_type: String,
    pub slot: u32,
}

/// Dispatch state for the event currently being delivered.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DispatchState {
    pub prevent_default: bool,
    pub stop_propagation: bool,
    pub stop_immediate: bool,
    /// The listener currently running ignores `preventDefault` (`{passive: true}`).
    pub passive: bool,
}

/// A scheduled timer.
#[derive(Clone, Debug)]
pub(crate) struct TimerEntry {
    pub id: u64,
    pub due_ms: u64,
    pub interval_ms: Option<u64>,
    /// `requestAnimationFrame` callback: invoked with the current virtual timestamp and
    /// never rescheduled (`pending_animation_frames` counts them).
    pub animation: bool,
}

/// Viewport/scroll state exposed to scripts (`window.innerWidth`, page offsets, ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportInfo {
    /// Viewport width in CSS pixels (`window.innerWidth`).
    pub width: u32,
    /// Viewport height in CSS pixels (`window.innerHeight`).
    pub height: u32,
    /// Device pixel ratio (`window.devicePixelRatio`).
    pub device_pixel_ratio: f64,
    /// Horizontal scroll offset (`window.pageXOffset`).
    pub scroll_x: f64,
    /// Vertical scroll offset (`window.pageYOffset`).
    pub scroll_y: f64,
}

impl Default for ViewportInfo {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            device_pixel_ratio: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

/// History API action forwarded to the host provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    /// `history.pushState(state, title, url)` (state is JSON; url already resolved).
    Push { state: String, url: Option<String> },
    /// `history.replaceState(...)`.
    Replace { state: String, url: Option<String> },
    /// `history.back()`.
    Back,
    /// `history.forward()`.
    Forward,
    /// `history.go(delta)`.
    Go(i64),
}

/// `location.assign/replace/reload` action forwarded to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocationAction {
    Assign(String),
    Replace(String),
    Reload,
}

/// Which Web Storage area an operation refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageKind {
    Local,
    Session,
}

/// Viewport-space rectangle (CSS pixels) for one DOM node, as supplied by the host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRect {
    /// Border-box `(x, y, width, height)` in viewport coordinates.
    pub border_box: (f32, f32, f32, f32),
    /// Content-box `(x, y, width, height)` in viewport coordinates.
    pub content_box: (f32, f32, f32, f32),
}

/// One `fetch()` request normalized by the JS prelude (host executes the I/O).
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub redirect: Option<String>,
}

/// Host result for one `fetch()` request.
#[derive(Clone, Debug)]
pub struct FetchResult {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// An event delivered by a WebSocket transport (#1534).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketEvent {
    /// The connection was established.
    Open,
    /// A text message arrived.
    Message(String),
    /// A binary message arrived (#1711). Delivered to JS as an `ArrayBuffer`.
    Binary(Vec<u8>),
    /// The connection closed with the given code.
    Close(u16),
    /// A transport error occurred.
    Error(String),
}

/// Host-owned WebSocket transport: the JS side sends text and polls for incoming events.
/// The real implementation (in the shell) drives `megi_net::websocket::WebSocketClient`.
pub trait WebSocketTransport: Send {
    fn send(&mut self, text: &str) -> Result<(), crate::error::JsError>;
    /// Sends a binary message (WHATWG WebSockets §9.3 / RFC 6455 §5.6).
    fn send_binary(&mut self, bytes: &[u8]) -> Result<(), crate::error::JsError> {
        let _ = bytes;
        Err(crate::error::JsError::Runtime(
            "WebSocket binary send is not supported by this transport".to_owned(),
        ))
    }
    fn poll(&mut self) -> Vec<WebSocketEvent>;
    fn close(&mut self);
}

/// Parsed `<iframe sandbox="...">` permission flags (#1544).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxFlags {
    pub allow_scripts: bool,
    pub allow_same_origin: bool,
    pub allow_forms: bool,
    pub allow_popups: bool,
    pub allow_top_navigation: bool,
}

impl Default for SandboxFlags {
    fn default() -> Self {
        Self {
            allow_scripts: true,
            allow_same_origin: true,
            allow_forms: true,
            allow_popups: true,
            allow_top_navigation: true,
        }
    }
}

impl SandboxFlags {
    /// Parses a whitespace-separated `<iframe sandbox>` attribute value.
    /// When `sandbox` is present (even empty), all capabilities start disabled
    /// and must be explicitly re-enabled by tokens.
    pub fn from_attribute(tokens: &str) -> Self {
        let mut flags = Self {
            allow_scripts: false,
            allow_same_origin: false,
            allow_forms: false,
            allow_popups: false,
            allow_top_navigation: false,
        };
        for token in tokens.split_ascii_whitespace() {
            if token.eq_ignore_ascii_case("allow-scripts") {
                flags.allow_scripts = true;
            } else if token.eq_ignore_ascii_case("allow-same-origin") {
                flags.allow_same_origin = true;
            } else if token.eq_ignore_ascii_case("allow-forms") {
                flags.allow_forms = true;
            } else if token.eq_ignore_ascii_case("allow-popups") {
                flags.allow_popups = true;
            } else if token.eq_ignore_ascii_case("allow-top-navigation")
                || token.eq_ignore_ascii_case("allow-top-navigation-by-user-activation")
            {
                flags.allow_top_navigation = true;
            }
        }
        flags
    }
}

/// Computes the serialized origin (`"scheme://host[:port]"` or `"null"` when opaque/non-network).
pub fn origin_for_url(url: &str, opaque: bool) -> String {
    if opaque {
        return "null".to_string();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return "null".to_string();
    };
    let lower_scheme = scheme.to_ascii_lowercase();
    if !matches!(lower_scheme.as_str(), "http" | "https") {
        return "null".to_string();
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return "null".to_string();
    }
    let normalized = if let Some((host, port)) = authority.rsplit_once(':')
        && !authority.ends_with(']')
    {
        if (lower_scheme == "http" && port == "80") || (lower_scheme == "https" && port == "443") {
            host.to_ascii_lowercase()
        } else {
            format!("{}:{}", host.to_ascii_lowercase(), port)
        }
    } else {
        authority.to_ascii_lowercase()
    };
    format!("{lower_scheme}://{normalized}")
}

/// Returns whether `child_url` is same-origin with `parent_url` given `opaque` sandbox status.
pub fn is_same_origin_url(parent_url: &str, child_url: &str, opaque: bool) -> bool {
    if opaque {
        return false;
    }
    if child_url == "about:blank" || child_url.is_empty() {
        return true;
    }
    let parent_scheme = parent_url
        .split_once("://")
        .map(|(s, _)| s.to_ascii_lowercase());
    let child_scheme = child_url
        .split_once("://")
        .map(|(s, _)| s.to_ascii_lowercase());
    match (parent_scheme.as_deref(), child_scheme.as_deref()) {
        (Some("file"), Some("file")) => true,
        (Some("http" | "https"), Some("http" | "https")) => {
            origin_for_url(parent_url, false) == origin_for_url(child_url, false)
        }
        _ => parent_url == child_url,
    }
}

/// Optional persistence provider: load is called once when installed, save after writes.
pub struct StorageProvider {
    pub load: StorageLoadFn,
    pub save: StorageSaveFn,
}

/// Injectable host cookie store backing `document.cookie`.
pub struct CookieProvider {
    pub get: CookieGetFn,
    pub set: CookieSetFn,
}

/// Maximum simultaneous `MutationObserver` registrations per realm (#1134).
pub(crate) const MAX_OBSERVERS: usize = 1024;
/// Distinct nodes a single `MutationObserver` may track (#437).
pub(crate) const MAX_OBSERVER_TARGETS: usize = 1024;
/// Maximum queued `MutationRecord`s per observer between deliveries.
pub(crate) const MAX_OBSERVER_RECORDS: usize = 4096;

/// One queued MutationObserver record.
#[derive(Clone, Debug)]
pub(crate) struct MutationRecord {
    pub kind: MutationKind,
    pub target: NodeId,
    pub name: Option<String>,
    pub old_value: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutationKind {
    ChildList,
    Attributes,
    CharacterData,
}

/// A `MutationObserver` registration (callbacks/records live in JS-side roots).
#[derive(Clone, Debug)]
pub(crate) struct ObserverRegistration {
    pub callback_slot: u32,
    pub object_slot: u32,
    pub targets: Vec<ObserverTarget>,
    pub records: Vec<MutationRecord>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ObserverTarget {
    pub node: NodeId,
    pub child_list: bool,
    pub attributes: bool,
    pub attribute_old_value: bool,
    pub character_data: bool,
    pub subtree: bool,
}

impl ObserverTarget {
    pub(crate) fn matches_kind(self, kind: MutationKind) -> bool {
        match kind {
            MutationKind::ChildList => self.child_list,
            MutationKind::Attributes => self.attributes,
            MutationKind::CharacterData => self.character_data,
        }
    }
}

/// Insertion-order-preserving key-value map for Web Storage (`localStorage`/`sessionStorage`)
/// so `storage.key(index)` is deterministic (#571).
#[derive(Clone, Debug, Default)]
pub struct StorageMap {
    /// O(1) lookups; the 1 MiB storage cap bounds the number of entries.
    values: HashMap<String, String>,
    /// Insertion order for deterministic `Storage.key(index)` (#571).
    order: Vec<String>,
}

impl StorageMap {
    /// Returns the value for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    /// Inserts or updates `key` with `value`, preserving insertion order for existing keys.
    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        if let Some(existing) = self.values.get_mut(&key) {
            return Some(std::mem::replace(existing, value));
        }
        self.order.push(key.clone());
        self.values.insert(key, value)
    }

    /// Removes `key`, returning its value if present.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let value = self.values.remove(key)?;
        if let Some(pos) = self.order.iter().position(|existing| existing == key) {
            self.order.remove(pos);
        }
        Some(value)
    }

    /// Clears all entries.
    pub fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.order.iter()
    }

    /// Iterates `(key, value)` pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order.iter().map(|key| (key, &self.values[key]))
    }
}

impl FromIterator<(String, String)> for StorageMap {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        let mut map = Self::default();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

/// Shared, mutable host state.
pub const MAX_CONSOLE_ENTRIES: usize = 4096;
