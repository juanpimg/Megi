//! Real winit window with a softbuffer CPU framebuffer.
//!
//! **This module creates an `EventLoop` and must never run in tests.** The unit
//! and integration tests exercise the pure logic in [`crate::cli`],
//! [`crate::convert`], [`crate::scroll`], [`crate::scale`], [`crate::present`]
//! and [`crate::worker`]; CI is headless (`--screenshot`) and never opens a
//! window.
//!
//! Behaviour:
//! - the page is rendered by a **worker thread** at the CSS viewport size and
//!   full content height (capped by [`crate::scroll::MAX_CONTENT_HEIGHT`]); the
//!   event loop never blocks on the render pipeline;
//! - resize and HiDPI scale changes are debounced (~120 ms) before requesting a
//!   new render; results of obsolete requests are discarded by generation;
//! - vertical scrolling presents the visible region (mouse wheel and
//!   arrows/PageUp/PageDown/Home/End);
//! - **link navigation**: left click on a rendered `<a href>` navigates to the resolved
//!   URL through the history state ([`crate::navigation`]) and a fresh worker render; the
//!   hit regions come from the same render pass as the pixels ([`crate::links`]);
//! - **history**: `Alt+←`/`Backspace`/`b` go back, `Alt+→`/`n` go forward (bounded by
//!   [`crate::MAX_HISTORY`]), `r` reloads the current page; failed navigations show an
//!   error frame and keep the history entry;
//! - **zoom** (WP-7.6): `Ctrl+=`/`Ctrl++`, `Ctrl+-`, `Ctrl+0` (also without Ctrl) walk the
//!   fixed 50–300 % ladder ([`crate::zoom`]); the CSS viewport becomes
//!   `physical / (device_scale * zoom)` and the title shows the zoom percentage;
//! - **auto-refresh** (WP-7.6): with `--refresh-ms` the window re-renders while the last
//!   frame executed scripts and left page timers pending (`setInterval`, long timer chains),
//!   preserving the scroll offset and using the worker's generation cancellation; pages
//!   without timers never refresh ([`crate::refresh`]);
//! - **chrome**: a shell-drawn overlay with a tab strip (activate/close/new + overflow) and
//!   an address bar ([`crate::chrome`], `Ctrl+L`); plus the `<select>` dropdown;
//! - **dynamic title/status**: the window title shows the document `<title>` plus the short
//!   URL, `zoom%`, `scroll%`, warning/script-error counters ([`crate::title`]);
//! - errors produce a white frame plus stderr/title feedback; the window stays
//!   usable.
//!
//! Controls (window mode):
//!
//! | Input | Action |
//! |---|---|
//! | Left click on a link | Navigate to its resolved URL (history push) |
//! | `Alt+←`, `Backspace`, `b` | Back |
//! | `Alt+→`, `n` | Forward |
//! | `r` | Reload the current page |
//! | `Ctrl+=` / `Ctrl++` (or `=`/`+`) | Zoom in |
//! | `Ctrl+-` (or `-`) | Zoom out |
//! | `Ctrl+0` (or `0`) | Reset zoom to 100 % |
//! | `Ctrl+L` | Edit the address bar (`Enter` navigates, `Esc` cancels) |
//! | `Ctrl+S` | Save the rendered page as a PNG download |
//! | `Ctrl+Shift+S` | Save the rendered DOM as an HTML download (doctype included) |
//! | `Ctrl+F` | Find in page (type, `Enter` next, `Shift+Enter` previous, `Esc` close) |
//! | `Ctrl+D` | Print the final URL and page title |
//! | `F5`, `g` | Reload |
//! | `Alt+Home` | Go to the first (home) history entry |
//! | `Ctrl+T` / `Ctrl+W` | New tab / close tab (last tab resets to New Tab) |
//! | `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
//! | `Alt+1`..`Alt+9` | Activate a tab by position |
//! | Wheel / touchpad | Scroll vertically |
//! | `↑` / `↓` | Scroll one line |
//! | `PageUp` / `PageDown` | Scroll one page |
//! | `Home` / `End` | Top / bottom |
//! | `q`, `Esc`, close button | Quit |
//!
//! Right/middle clicks are ignored. Fragment links are treated as a normal navigation in
//! this round (no in-page scrolling), and non-navigable schemes (`javascript:`, `mailto:`,
//! ...) are no-ops.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::a11y;
use crate::chrome::{
    self, AddressBar, ChromeView, ContextMenu, DropdownState, MenuAction, NavigationButton, TabHit,
};
use crate::convert::{WHITE_0RGB, expected_buffer_len};
use crate::error::ShellError;
use crate::find::{self, FindState};
use crate::forms::{self, FocusState, FormControl, FormControlKind, TextEdit};
use crate::history::{HistoryMenu, RecentHistory, is_history_url};
use crate::links::{LinkRegion, link_url_at, physical_to_page, resolve_href};
use crate::navigation::{NavigationOutcome, NavigationState};
use crate::present;
use crate::refresh::{RefreshPolicy, RefreshTimer};
use crate::scale::{WindowMetrics, window_metrics_zoomed};
use crate::scroll::{self, ScrollState};
use crate::tabs::{self, CloseOutcome, TabsState};
use crate::title::{self, StatusSummary};
use crate::worker::{
    Debounce, FrameStore, MAX_SITE_RENDERERS, RENDER_DEBOUNCE, RenderRequest, RendererPool,
    SessionAutosave, Worker, spawn_worker,
};
use crate::zoom;

/// Builds `blur` / `focus` [`megi_webview::render::HostDomEvent`] sequence when shell focus moves (#1504).
fn host_focus_events(
    prev_focus: Option<u32>,
    next_focus: Option<u32>,
) -> Vec<megi_webview::render::HostDomEvent> {
    if prev_focus == next_focus {
        return Vec::new();
    }
    let mut events = Vec::new();
    if let Some(prev) = prev_focus {
        events.push(megi_webview::render::HostDomEvent {
            event_type: "blur".to_owned(),
            target: megi_webview::render::HostEventTarget::FormControl(prev),
            bubbles: false,
            cancelable: false,
            kind: megi_webview::render::HostUiEventKind::Focus,
        });
    }
    if let Some(next) = next_focus {
        events.push(megi_webview::render::HostDomEvent {
            event_type: "focus".to_owned(),
            target: megi_webview::render::HostEventTarget::FormControl(next),
            bubbles: false,
            cancelable: false,
            kind: megi_webview::render::HostUiEventKind::Focus,
        });
    }
    events
}

/// Builds a bubbling, cancelable `keydown` [`megi_webview::render::HostDomEvent`] from a `winit` key (#1504).
fn host_keydown_event(
    target: megi_webview::render::HostEventTarget,
    key: &Key,
    ctrl_key: bool,
    shift_key: bool,
    alt_key: bool,
) -> Option<megi_webview::render::HostDomEvent> {
    let (dom_key, code) = dom_key_and_code(key)?;
    Some(megi_webview::render::HostDomEvent {
        event_type: "keydown".to_owned(),
        target,
        bubbles: true,
        cancelable: true,
        kind: megi_webview::render::HostUiEventKind::Keyboard {
            key: dom_key,
            code,
            ctrl_key,
            shift_key,
            alt_key,
            meta_key: false,
            repeat: false,
        },
    })
}

/// Maps a `winit` logical [`Key`] to DOM `(KeyboardEvent.key, KeyboardEvent.code)`.
fn dom_key_and_code(key: &Key) -> Option<(String, String)> {
    match key {
        Key::Named(NamedKey::Enter) => Some(("Enter".to_owned(), "Enter".to_owned())),
        Key::Named(NamedKey::Backspace) => Some(("Backspace".to_owned(), "Backspace".to_owned())),
        Key::Named(NamedKey::Tab) => Some(("Tab".to_owned(), "Tab".to_owned())),
        Key::Named(NamedKey::Escape) => Some(("Escape".to_owned(), "Escape".to_owned())),
        Key::Named(NamedKey::Space) => Some((" ".to_owned(), "Space".to_owned())),
        Key::Named(NamedKey::ArrowLeft) => Some(("ArrowLeft".to_owned(), "ArrowLeft".to_owned())),
        Key::Named(NamedKey::ArrowRight) => {
            Some(("ArrowRight".to_owned(), "ArrowRight".to_owned()))
        }
        Key::Named(NamedKey::ArrowUp) => Some(("ArrowUp".to_owned(), "ArrowUp".to_owned())),
        Key::Named(NamedKey::ArrowDown) => Some(("ArrowDown".to_owned(), "ArrowDown".to_owned())),
        Key::Character(text) => {
            let s = text.as_str();
            if s.is_empty() {
                return None;
            }
            let code = match s.chars().next() {
                Some(' ') if s.len() == 1 => "Space".to_owned(),
                Some(ch) if ch.is_ascii_alphabetic() && s.len() == 1 => {
                    format!("Key{}", ch.to_ascii_uppercase())
                }
                Some(ch) if ch.is_ascii_digit() && s.len() == 1 => format!("Digit{ch}"),
                _ => String::new(),
            };
            Some((s.to_owned(), code))
        }
        _ => None,
    }
}

#[cfg(feature = "a11y")]
mod accessibility;
mod downloads;
mod events;
mod form_controls;
mod frontend;
mod input;
mod keyboard;
mod presentation;
mod save_as;
#[cfg(test)]
mod tests;
mod ui;
mod worker;

type ShellContext = softbuffer::Context<Arc<Window>>;
type ShellSurface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

/// Wake-up event sent by the render worker when it has a message.
#[derive(Debug)]
enum UserEvent {
    /// A worker message (frame or error) is ready on the channel.
    WorkerReady,
    /// A platform accessibility request forwarded by `accesskit_winit` (ADR-002).
    #[cfg(feature = "a11y")]
    Accessibility(accesskit_winit::Event),
}

#[cfg(feature = "a11y")]
impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        UserEvent::Accessibility(event)
    }
}

/// Shell-specific view settings for one window: page zoom and auto-refresh policy.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowViewOptions {
    /// Initial page zoom factor (clamped to 50–300 %).
    pub zoom: f64,
    /// Auto-refresh policy for pages with pending timers.
    pub refresh: RefreshPolicy,
    /// Initial tabs (empty = the single `input` tab passed to the runner).
    pub tabs: Vec<String>,
    /// Session file to load/save (`None` disables persistence).
    pub session_path: Option<std::path::PathBuf>,
    /// Download directory (`None` = `$HOME/Downloads`/temp fallback).
    pub download_dir: Option<std::path::PathBuf>,
    /// Use OS renderer processes with DOM-patching IPC instead of per-site threads.
    pub process_render: bool,
}

impl Default for WindowViewOptions {
    fn default() -> Self {
        Self {
            zoom: zoom::DEFAULT_ZOOM,
            refresh: RefreshPolicy::disabled(),
            tabs: Vec::new(),
            session_path: None,
            download_dir: None,
            process_render: true,
        }
    }
}

impl WindowViewOptions {
    /// View options with an explicit initial zoom and refresh policy.
    pub fn new(zoom: f64, refresh: RefreshPolicy) -> Self {
        Self {
            zoom,
            refresh,
            tabs: Vec::new(),
            session_path: None,
            download_dir: None,
            process_render: true,
        }
    }

    /// Enables OS renderer processes with DOM-patching IPC (ADR-004).
    pub fn with_process_render(mut self, enabled: bool) -> Self {
        self.process_render = enabled;
        self
    }

    /// Adds the initial tab list (`--tabs`, repeatable).
    pub fn with_tabs(mut self, tabs: Vec<String>) -> Self {
        self.tabs = tabs;
        self
    }
}

/// Runs the interactive window loop until the user closes it.
///
/// Returns [`ShellError::Window`] when no display server is available (no
/// `$DISPLAY` / `$WAYLAND_DISPLAY`) or when winit/softbuffer initialization
/// fails.
pub fn run_window(input: String, title: String, width: u32, height: u32) -> Result<(), ShellError> {
    run_window_with_view_options(
        input,
        title,
        width,
        height,
        megi_webview::render::RenderOptions::default(),
        WindowViewOptions::default(),
    )
}

/// [`run_window`] with explicit base [`megi_webview::render::RenderOptions`] (cookie jar, CORS origin,
/// scripting toggles) applied to every render of the session.
pub fn run_window_with_options(
    input: String,
    title: String,
    width: u32,
    height: u32,
    render_options: megi_webview::render::RenderOptions,
) -> Result<(), ShellError> {
    run_window_with_view_options(
        input,
        title,
        width,
        height,
        render_options,
        WindowViewOptions::default(),
    )
}

/// [`run_window_with_options`] plus zoom/auto-refresh view settings (WP-7.6).
pub fn run_window_with_view_options(
    input: String,
    title: String,
    width: u32,
    height: u32,
    render_options: megi_webview::render::RenderOptions,
    view: WindowViewOptions,
) -> Result<(), ShellError> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|error| {
            ShellError::Window(format!(
                "cannot create the event loop (is an X11/Wayland session available?): {error}"
            ))
        })?;
    let proxy = event_loop.create_proxy();
    let mut app = WindowApp::new(input, title, width, height, proxy, render_options, view);
    event_loop
        .run_app(&mut app)
        .map_err(|error| ShellError::Window(format!("event loop failed: {error}")))
}

/// One keyboard-focusable element, in document order (#1517).
///
/// Tab/Shift+Tab walk [`WindowApp::focus_targets`] instead of the form-control list alone,
/// so `<a href>` anchors are reachable without a pointer; `Enter` (and `Space`) activates a
/// focused link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusTarget {
    /// Form control identified by its stable `data-megi-form-id`.
    Control(u32),
    /// Anchor node (`<a href>`), resolved through [`LinkRegion::node`].
    Link(megi_webview::core::NodeId),
}

/// Builds the keyboard focus list for a DOM snapshot in document order (#1517).
/// Maps each focusable node to a form control or link; pure and window-free.
fn build_focus_targets(
    document: &megi_webview::core::Document,
    forms: &[FormControl],
    links: &[LinkRegion],
) -> Vec<FocusTarget> {
    // #2196: O(1) node indexes, not O(F*(A+L)) linear scans per frame.
    let form_by_node: std::collections::HashMap<_, _> = forms
        .iter()
        .map(|control| (control.node_id, control.form_id))
        .collect();
    let link_nodes: std::collections::HashSet<_> =
        links.iter().filter_map(|region| region.node).collect();
    a11y::focus_order(document, document.root())
        .into_iter()
        .map(|node| {
            form_by_node
                .get(&node)
                .map(|form_id| FocusTarget::Control(*form_id))
                .unwrap_or(FocusTarget::Link(node))
        })
        .filter(|target| match target {
            FocusTarget::Control(_) => true,
            FocusTarget::Link(node) => link_nodes.contains(node),
        })
        .collect()
}

struct WindowApp {
    input: String,
    title: String,
    requested_width: u32,
    requested_height: u32,
    window: Option<Arc<Window>>,
    context: Option<ShellContext>,
    surface: Option<ShellSurface>,
    proxy: EventLoopProxy<UserEvent>,
    /// Base render options for this session (cookie jar, CORS origin, scripting, ...).
    render_options: megi_webview::render::RenderOptions,
    metrics: WindowMetrics,
    scale_factor: f64,
    scroll: ScrollState,
    store: FrameStore,
    /// Per-site renderer pool (ADR-004 phase 2).
    render_pool: Option<RendererPool>,
    /// OS renderer process and its queued replies (ADR-004 phase 3).
    process_pool: Option<crate::ipc::ProcessRenderPool>,
    use_process_render: bool,
    process_messages: std::collections::VecDeque<crate::worker::WorkerMessage>,
    debounce: Debounce,
    pending_metrics: Option<WindowMetrics>,
    error: Option<String>,
    content_capped: bool,
    /// Session history for this window (click/back/forward/reload).
    nav: NavigationState,
    /// Clickable link regions of the currently displayed frame (page CSS coordinates).
    links: Vec<LinkRegion>,
    /// `<title>` of the currently displayed document.
    doc_title: Option<String>,
    /// Latest physical cursor position over the window.
    cursor: Option<(f64, f64)>,
    /// Current keyboard modifiers (for Alt+←/→ and Ctrl+zoom/L).
    modifiers: ModifiersState,
    /// Current page zoom factor (50–300 %).
    zoom: f64,
    /// Auto-refresh timer for pages with pending page timers.
    refresh: RefreshTimer,
    /// Scripts executed by the frame on screen.
    frame_scripts_executed: usize,
    /// Page timers left pending by the frame on screen (auto-refresh trigger).
    frame_timers_pending: usize,
    /// Non-fatal render warnings of the frame on screen.
    frame_warnings: usize,
    /// Script errors of the frame on screen.
    frame_script_errors: usize,
    /// Form controls of the frame on screen (page CSS coordinates).
    forms: Vec<FormControl>,
    /// Focus model over `focus_targets`.
    focus: FocusState,
    /// Keyboard-focusable elements in document order (form controls and `<a href>`),
    /// rebuilt on every applied frame (#1517).
    focus_targets: Vec<FocusTarget>,
    /// Text editor for the focused text control (`None` for buttons/checkboxes).
    editor: Option<TextEdit>,
    /// Active IME composition state (#1519).
    ime: crate::forms::ImeState,
    /// Visual Save-as dialog (F7-SAVE-AS), open when `Some`.
    save_dialog: Option<save_as::SaveDialogState>,
    /// DOM snapshot of the frame on screen (source for editing re-renders).
    document: Option<Arc<megi_webview::core::Document>>,
    /// Pending button id whose pressed state must be released after the next frame.
    release_button: Option<u32>,
    /// Find-in-page state (query, hits, current hit).
    find: FindState,
    /// Whether the find prompt is open (typing edits the query).
    find_editing: bool,
    /// Open tabs and the active one.
    tabs: TabsState,
    /// Highlight rectangles of the last frame (page CSS coordinates).
    find_rects: Vec<crate::links::CssRect>,
    /// Address bar state (Ctrl+L).
    address: AddressBar,
    /// Open `<select>` dropdown, if any.
    dropdown: Option<DropdownState>,
    /// Session file to load/save (`None` = persistence disabled).
    session_path: Option<std::path::PathBuf>,
    /// Download directory override.
    download_dir: Option<std::path::PathBuf>,
    /// Debounced session save (dirty flag + timer, #36).
    session_autosave: SessionAutosave,
    /// Whether unsaved session changes exist.
    /// Transient status notice shown in the window title (downloads/session).
    status: Option<String>,
    /// Render worker has a request in flight for the active tab (loading indicator).
    render_pending: bool,
    /// Open overflow menu for hidden tabs.
    overflow_menu: Option<crate::chrome::OverflowMenu>,
    /// URL of the link under the cursor (bottom status bar).
    hover_url: Option<String>,
    /// Open right-click context menu.
    context_menu: Option<ContextMenu>,
    /// Internal text clipboard shared by context-menu copy actions.
    clipboard: String,
    /// Platform accessibility adapter (ADR-002): publishes the shell snapshot to AT-SPI/UIA.
    #[cfg(feature = "a11y")]
    accessibility_adapter: Option<accesskit_winit::Adapter>,
    /// Pure state tracking which tree signature was last published.
    #[cfg(feature = "a11y")]
    accessibility_publisher: crate::a11y_backend::AccessibilityPublisher,
    /// Global recent history (newest first) for Ctrl+H and the internal history page.
    recent_history: RecentHistory,
    /// Open Ctrl+H history menu.
    history_menu: Option<HistoryMenu>,
    /// Download progress/last-download state shown in the status bar.
    download: crate::download::DownloadProgress,
    /// Download manager + panel UI state (F7-SAVE-AS).
    download_manager: crate::download::DownloadManager,
    download_panel: downloads::DownloadPanelState,
    /// Last clicked option of a multi-select list (Shift range anchor).
    multi_anchor: Option<(u32, usize)>,
    /// Form control under the mouse (native hover visual).
    hover_form: Option<usize>,
    /// Per-origin zoom memory (persisted in the session when enabled).
    site_zooms: Vec<(String, f64)>,
    /// Reader-mode data URL for the current `about:reader` view.
    reader_html: Option<String>,
}
