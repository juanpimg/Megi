//! Desktop window event loop and state (`winit` + `softbuffer`).
//!
//! Only compiled when the `window` feature is enabled.

pub(super) use std::num::NonZeroU32;
pub(super) use std::sync::Arc;
pub(super) use std::time::Instant;

pub(super) use winit::application::ApplicationHandler;
pub(super) use winit::dpi::LogicalSize;
pub(super) use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
pub(super) use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
pub(super) use winit::keyboard::{Key, ModifiersState, NamedKey};
pub(super) use winit::window::{Window, WindowId};

pub(super) use crate::a11y;
pub(super) use crate::chrome::{
    self, AddressBar, ChromeView, ContextMenu, DropdownState, MenuAction, NavigationButton, TabHit,
};
pub(super) use crate::convert::{WHITE_0RGB, expected_buffer_len};
pub(super) use crate::error::ShellError;
pub(super) use crate::find::{self, FindState};
pub(super) use crate::forms::{self, FocusState, FormControl, FormControlKind, TextEdit};
pub(super) use crate::history::{HistoryMenu, RecentHistory, is_history_url};
pub(super) use crate::links::{LinkRegion, link_url_at, physical_to_page, resolve_href};
pub(super) use crate::navigation::{NavigationOutcome, NavigationState};
pub(super) use crate::present;
pub(super) use crate::refresh::RefreshTimer;
pub(super) use crate::scale::{WindowMetrics, window_metrics_zoomed};
pub(super) use crate::scroll::{self, ScrollState};
pub(super) use crate::tabs::{self, CloseOutcome, TabsState};
pub(super) use crate::title::{self, StatusSummary};
pub(super) use crate::worker::{
    Debounce, FrameStore, MAX_SITE_RENDERERS, RENDER_DEBOUNCE, RenderRequest, RendererPool,
    SessionAutosave, Worker, spawn_worker,
};

use super::WindowViewOptions;

/// Builds `blur` / `focus` [`megi_webview::render::HostDomEvent`] sequence when shell focus moves (#1504).
pub(super) fn host_focus_events(
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
pub(super) fn host_keydown_event(
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

pub(super) type ShellContext = softbuffer::Context<Arc<Window>>;
pub(super) type ShellSurface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

/// Wake-up event sent by the render worker when it has a message.
#[derive(Debug)]
pub(super) enum UserEvent {
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

/// Runs the winit + softbuffer desktop event loop.
pub(super) fn run(
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
pub(super) enum FocusTarget {
    /// Form control identified by its stable `data-megi-form-id`.
    Control(u32),
    /// Anchor node (`<a href>`), resolved through [`LinkRegion::node`].
    Link(megi_webview::core::NodeId),
}

/// Builds the keyboard focus list for a DOM snapshot in document order (#1517).
/// Maps each focusable node to a form control or link; pure and window-free.
pub(super) fn build_focus_targets(
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

pub(super) struct WindowApp {
    pub(super) input: String,
    pub(super) title: String,
    pub(super) requested_width: u32,
    pub(super) requested_height: u32,
    pub(super) window: Option<Arc<Window>>,
    pub(super) context: Option<ShellContext>,
    pub(super) surface: Option<ShellSurface>,
    pub(super) proxy: EventLoopProxy<UserEvent>,
    /// Base render options for this session (cookie jar, CORS origin, scripting, ...).
    pub(super) render_options: megi_webview::render::RenderOptions,
    pub(super) metrics: WindowMetrics,
    pub(super) scale_factor: f64,
    pub(super) scroll: ScrollState,
    pub(super) store: FrameStore,
    /// Per-site renderer pool (ADR-004 phase 2).
    pub(super) render_pool: Option<RendererPool>,
    /// OS renderer process and its queued replies (ADR-004 phase 3).
    pub(super) process_pool: Option<crate::ipc::ProcessRenderPool>,
    pub(super) use_process_render: bool,
    pub(super) process_messages: std::collections::VecDeque<crate::worker::WorkerMessage>,
    pub(super) debounce: Debounce,
    /// Coalesces find-in-page query changes into one render (#1830).
    pub(super) find_render: crate::worker::FindRenderCoalescer,
    pub(super) pending_metrics: Option<WindowMetrics>,
    pub(super) error: Option<String>,
    pub(super) content_capped: bool,
    /// Session history for this window (click/back/forward/reload).
    pub(super) nav: NavigationState,
    /// Clickable link regions of the currently displayed frame (page CSS coordinates).
    pub(super) links: Vec<LinkRegion>,
    /// `<title>` of the currently displayed document.
    pub(super) doc_title: Option<String>,
    /// Latest physical cursor position over the window.
    pub(super) cursor: Option<(f64, f64)>,
    /// Current keyboard modifiers (for Alt+←/→ and Ctrl+zoom/L).
    pub(super) modifiers: ModifiersState,
    /// Current page zoom factor (50–300 %).
    pub(super) zoom: f64,
    /// Auto-refresh timer for pages with pending page timers.
    pub(super) refresh: RefreshTimer,
    /// Scripts executed by the frame on screen.
    pub(super) frame_scripts_executed: usize,
    /// Page timers left pending by the frame on screen (auto-refresh trigger).
    pub(super) frame_timers_pending: usize,
    /// Non-fatal render warnings of the frame on screen.
    pub(super) frame_warnings: usize,
    /// Script errors of the frame on screen.
    pub(super) frame_script_errors: usize,
    /// Form controls of the frame on screen (page CSS coordinates).
    pub(super) forms: Vec<FormControl>,
    /// Focus model over `focus_targets`.
    pub(super) focus: FocusState,
    /// Keyboard-focusable elements in document order (form controls and `<a href>`),
    /// rebuilt on every applied frame (#1517).
    pub(super) focus_targets: Vec<FocusTarget>,
    /// Text editor for the focused text control (`None` for buttons/checkboxes).
    pub(super) editor: Option<TextEdit>,
    /// Active IME composition state (#1519).
    pub(super) ime: crate::forms::ImeState,
    /// Visual Save-as dialog (F7-SAVE-AS), open when `Some`.
    pub(super) save_dialog: Option<super::save_as::SaveDialogState>,
    /// DOM snapshot of the frame on screen (source for editing re-renders).
    pub(super) document: Option<Arc<megi_webview::core::Document>>,
    /// O(1) in-place DOM form patcher and JS UI-event debouncer (#2199).
    pub(super) live_forms: crate::forms::LiveFormPatcher,
    /// Pending button id whose pressed state must be released after the next frame.
    pub(super) release_button: Option<u32>,
    /// Find-in-page state (query, hits, current hit).
    pub(super) find: FindState,
    /// Whether the find prompt is open (typing edits the query).
    pub(super) find_editing: bool,
    /// Open tabs and the active one.
    pub(super) tabs: TabsState,
    /// Highlight rectangles of the last frame (page CSS coordinates).
    pub(super) find_rects: Vec<crate::links::CssRect>,
    /// Address bar state (Ctrl+L).
    pub(super) address: AddressBar,
    /// Open `<select>` dropdown, if any.
    pub(super) dropdown: Option<DropdownState>,
    /// Session file to load/save (`None` = persistence disabled).
    pub(super) session_path: Option<std::path::PathBuf>,
    /// Download directory override.
    pub(super) download_dir: Option<std::path::PathBuf>,
    /// Debounced session save (dirty flag + timer, #36).
    pub(super) session_autosave: SessionAutosave,
    /// Transient status notice shown in the window title (downloads/session).
    pub(super) status: Option<String>,
    /// Render worker has a request in flight for the active tab (loading indicator).
    pub(super) render_pending: bool,
    /// Open overflow menu for hidden tabs.
    pub(super) overflow_menu: Option<crate::chrome::OverflowMenu>,
    /// URL of the link under the cursor (bottom status bar).
    pub(super) hover_url: Option<String>,
    /// Open right-click context menu.
    pub(super) context_menu: Option<ContextMenu>,
    /// Internal text clipboard shared by context-menu copy actions.
    pub(super) clipboard: String,
    /// Platform accessibility adapter (ADR-002): publishes the shell snapshot to AT-SPI/UIA.
    #[cfg(feature = "a11y")]
    pub(super) accessibility_adapter: Option<accesskit_winit::Adapter>,
    /// Pure state tracking which tree signature was last published.
    #[cfg(feature = "a11y")]
    pub(super) accessibility_publisher: crate::a11y_backend::AccessibilityPublisher,
    /// Global recent history (newest first) for Ctrl+H and the internal history page.
    pub(super) recent_history: RecentHistory,
    /// Open Ctrl+H history menu.
    pub(super) history_menu: Option<HistoryMenu>,
    /// Download progress/last-download state shown in the status bar.
    pub(super) download: crate::download::DownloadProgress,
    /// Download manager + panel UI state (F7-SAVE-AS).
    pub(super) download_manager: crate::download::DownloadManager,
    pub(super) download_panel: super::downloads::DownloadPanelState,
    /// Last clicked option of a multi-select list (Shift range anchor).
    pub(super) multi_anchor: Option<(u32, usize)>,
    /// Form control under the mouse (native hover visual).
    pub(super) hover_form: Option<usize>,
    /// Per-origin zoom memory (persisted in the session when enabled).
    pub(super) site_zooms: Vec<(String, f64)>,
    /// Reader-mode data URL for the current `about:reader` view.
    pub(super) reader_html: Option<String>,
}
