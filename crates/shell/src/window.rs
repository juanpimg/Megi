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
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::chrome::{
    self, AddressBar, ChromeView, ContextMenu, DropdownState, MenuAction, NavigationButton, TabHit,
};
use crate::convert::{WHITE_0RGB, expected_buffer_len};
use crate::error::ShellError;
use crate::find::{self, FindState};
use crate::forms::{self, FocusState, FormControl, FormControlKind, TextEdit};
use crate::history::{HistoryMenu, RecentHistory, is_history_url};
use crate::links::{LinkRegion, link_url_at, physical_to_page};
use crate::navigation::{NavigationOutcome, NavigationState};
use crate::present;
use crate::refresh::{RefreshPolicy, RefreshTimer};
use crate::scale::{WindowMetrics, window_metrics_zoomed};
use crate::scroll::{self, ScrollState};
use crate::tabs::{self, CloseOutcome, TabsState};
use crate::title::{self, StatusSummary};
use crate::worker::{
    Debounce, FrameStore, RENDER_DEBOUNCE, RenderRequest, WorkerMessage, spawn_worker,
};
use crate::zoom;

type ShellContext = softbuffer::Context<Arc<Window>>;
type ShellSurface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

/// Wake-up event sent by the render worker when it has a message.
#[derive(Clone, Copy, Debug)]
enum UserEvent {
    /// A worker message (frame or error) is ready on the channel.
    WorkerReady,
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
}

impl Default for WindowViewOptions {
    fn default() -> Self {
        Self {
            zoom: zoom::DEFAULT_ZOOM,
            refresh: RefreshPolicy::disabled(),
            tabs: Vec::new(),
            session_path: None,
            download_dir: None,
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
        }
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
        render::RenderOptions::default(),
        WindowViewOptions::default(),
    )
}

/// [`run_window`] with explicit base [`render::RenderOptions`] (cookie jar, CORS origin,
/// scripting toggles) applied to every render of the session.
pub fn run_window_with_options(
    input: String,
    title: String,
    width: u32,
    height: u32,
    render_options: render::RenderOptions,
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
    render_options: render::RenderOptions,
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
    render_options: render::RenderOptions,
    metrics: WindowMetrics,
    scale_factor: f64,
    scroll: ScrollState,
    store: FrameStore,
    request_sender: Option<Sender<RenderRequest>>,
    message_receiver: Option<Receiver<WorkerMessage>>,
    worker: Option<JoinHandle<()>>,
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
    /// Focus model over `forms`.
    focus: FocusState,
    /// Text editor for the focused text control (`None` for buttons/checkboxes).
    editor: Option<TextEdit>,
    /// DOM snapshot of the frame on screen (source for editing re-renders).
    document: Option<Arc<dom::Document>>,
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
    /// Debounced session save.
    session_debounce: Debounce,
    /// Whether unsaved session changes exist.
    session_dirty: bool,
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
    /// Global recent history (newest first) for Ctrl+H and the internal history page.
    recent_history: RecentHistory,
    /// Open Ctrl+H history menu.
    history_menu: Option<HistoryMenu>,
}

impl WindowApp {
    fn new(
        input: String,
        title: String,
        width: u32,
        height: u32,
        proxy: EventLoopProxy<UserEvent>,
        render_options: render::RenderOptions,
        view: WindowViewOptions,
    ) -> Self {
        let mut startup_notice = None;
        let mut recent = RecentHistory::new();
        let tabs = if let Some(path) = &view.session_path {
            let loaded = crate::session::load(path);
            if let Some(warning) = &loaded.warning {
                eprintln!("megi: {warning}");
                startup_notice = Some(warning.clone());
            }
            match loaded.session {
                Some(session) => {
                    startup_notice = Some(format!("restored {} tab(s)", session.tabs.len()));
                    recent = crate::history::RecentHistory::from_entries(session.recent.clone());
                    session.restore()
                }
                None => TabsState::from_urls(view.tabs.clone()),
            }
        } else if view.tabs.is_empty() {
            TabsState::new(input.clone())
        } else {
            TabsState::from_urls(view.tabs.clone())
        };
        let input = tabs.active().url.clone();
        let nav = tabs.active().navigation.clone();
        let zoom = zoom::sanitize_zoom(tabs.active().zoom);
        let restored_scroll = tabs.active().scroll;
        Self {
            input,
            title,
            requested_width: width,
            requested_height: height,
            window: None,
            context: None,
            surface: None,
            proxy,
            render_options,
            metrics: window_metrics_zoomed(width, height, 1.0, zoom),
            scale_factor: 1.0,
            scroll: restored_scroll,
            store: FrameStore::new(),
            request_sender: None,
            message_receiver: None,
            worker: None,
            debounce: Debounce::new(RENDER_DEBOUNCE),
            pending_metrics: None,
            error: None,
            content_capped: false,
            nav,
            links: Vec::new(),
            doc_title: None,
            cursor: None,
            modifiers: ModifiersState::default(),
            zoom,
            refresh: RefreshTimer::new(view.refresh),
            frame_scripts_executed: 0,
            frame_timers_pending: 0,
            frame_warnings: 0,
            frame_script_errors: 0,
            forms: Vec::new(),
            focus: FocusState::new(),
            editor: None,
            document: None,
            release_button: None,
            find: FindState::new(),
            find_editing: false,
            find_rects: Vec::new(),
            address: AddressBar::new(),
            dropdown: None,
            session_path: view.session_path.clone(),
            download_dir: view.download_dir.clone(),
            session_debounce: Debounce::new(std::time::Duration::from_millis(600)),
            session_dirty: false,
            status: startup_notice,
            render_pending: false,
            overflow_menu: None,
            hover_url: None,
            context_menu: None,
            clipboard: String::new(),
            recent_history: recent,
            history_menu: None,
            tabs,
        }
    }

    /// Creates the window, surface and worker thread on first resume.
    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), ShellError> {
        if self.window.is_some() {
            return Ok(());
        }
        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(self.requested_width),
                f64::from(self.requested_height),
            ));
        let window =
            Arc::new(event_loop.create_window(attributes).map_err(|error| {
                ShellError::Window(format!("cannot create the window: {error}"))
            })?);
        let context = softbuffer::Context::new(Arc::clone(&window))
            .map_err(|error| ShellError::Window(format!("softbuffer context: {error}")))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window))
            .map_err(|error| ShellError::Window(format!("softbuffer surface: {error}")))?;

        self.scale_factor = window.scale_factor();
        let size = window.inner_size();
        self.metrics = window_metrics_zoomed(size.width, size.height, self.scale_factor, self.zoom);
        self.scroll = ScrollState::new(1, self.metrics.css_height);
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);

        let proxy = self.proxy.clone();
        let render_options = self.render_options.clone();
        let (sender, receiver, handle) = spawn_worker(
            move |request: &RenderRequest| {
                crate::worker::render_document_with_options(request, &render_options)
            },
            move || {
                // Wake the event loop; ignore a closed loop (window closing).
                let _ = proxy.send_event(UserEvent::WorkerReady);
            },
        );
        self.request_sender = Some(sender);
        self.message_receiver = Some(receiver);
        self.worker = Some(handle);
        Ok(())
    }

    /// Enqueues a full-page render for the current viewport.
    fn request_render(&mut self) {
        let Some(sender) = &self.request_sender else {
            self.error = Some("render worker is not running".to_string());
            self.update_title();
            return;
        };
        let render_input = if is_history_url(&self.input) {
            let html = crate::history::history_page_html(&self.tabs.active().history_log.recent());
            format!(
                "data:text/html;base64,{}",
                net::base64_encode(html.as_bytes())
            )
        } else {
            self.input.clone()
        };
        let request = if self.find.query().is_empty() {
            self.store.begin_request(
                &render_input,
                self.metrics.css_width,
                self.metrics.css_height,
            )
        } else {
            self.store.begin_find_request(
                &render_input,
                self.metrics.css_width,
                self.metrics.css_height,
                self.find.query(),
            )
        };
        self.render_pending = true;
        if sender.send(request).is_err() {
            self.error = Some("render worker stopped unexpectedly".to_string());
            self.update_title();
        }
    }

    /// Starts rendering `url` and resets per-page state (scroll, stale links/title).
    fn start_navigation(&mut self, url: &net::Url) {
        self.input = url.as_str();
        self.error = None;
        self.content_capped = false;
        self.links.clear();
        self.refresh.clear();
        self.frame_scripts_executed = 0;
        self.frame_timers_pending = 0;
        self.frame_warnings = 0;
        self.frame_script_errors = 0;
        self.forms.clear();
        self.focus.blur();
        self.editor = None;
        self.document = None;
        self.doc_title = None;
        self.close_find();
        self.dropdown = None;
        self.overflow_menu = None;
        self.context_menu = None;
        self.history_menu = None;
        self.hover_url = None;
        self.address.cancel();
        self.scroll = ScrollState::new(1, self.metrics.css_height);
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_render();
    }

    /// Commits a navigation (link click): pushes a history entry unless it is the same URL.
    fn navigate_to(&mut self, url: net::Url) {
        if self.nav.navigate_to(url) == NavigationOutcome::Pushed
            && let Some(current) = self.nav.current().cloned()
        {
            self.start_navigation(&current);
        }
    }

    /// Goes back one history entry (no-op at the beginning of the history).
    fn go_back(&mut self) {
        if let Some(url) = self.nav.back().cloned() {
            self.start_navigation(&url);
        }
    }

    /// Goes forward one history entry (no-op at the end of the history).
    fn go_forward(&mut self) {
        if let Some(url) = self.nav.forward().cloned() {
            self.start_navigation(&url);
        }
    }

    /// Reloads the current page (keeps scroll position and history).
    fn reload(&mut self) {
        if self.nav.reload_target().is_some() {
            self.request_render();
        }
    }

    /// Handles a left click: resolves the link under the cursor and navigates to it.
    /// Converts a physical cursor position into page CSS coordinates (scroll/zoom aware).
    fn page_coordinates(&self, physical_x: f64, physical_y: f64) -> (f32, f32) {
        physical_to_page(
            physical_x,
            physical_y,
            self.scale_factor,
            self.scroll.offset(),
        )
    }

    /// Updates the bottom status-bar hover URL and the context-menu hover selection.
    fn update_hover(&mut self, physical_x: f64, physical_y: f64) {
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let link = match crate::links::link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => Some(url.as_str()),
            _ => None,
        };
        let changed = link != self.hover_url;
        self.hover_url = link;
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        if let Some(menu) = self.context_menu.as_mut() {
            let layout = chrome::context_menu_layout(
                menu,
                self.metrics.css_width as f32,
                self.metrics.css_height,
            );
            if let Some(index) = chrome::context_menu_hit(&layout, menu_x, menu_y) {
                menu.selected = index;
            }
        }
        if changed {
            if let Some(window) = &self.window {
                window.set_cursor(if self.hover_url.is_some() {
                    winit::window::CursorIcon::Pointer
                } else {
                    winit::window::CursorIcon::Default
                });
            }
            self.request_repaint();
        }
    }

    /// Left click: context menu/overflow first, then Ctrl+click (new tab) or normal navigation.
    fn left_click(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        if self.history_menu.is_some() {
            let menu = self.history_menu.clone().expect("menu");
            let layout = chrome::history_menu_layout(
                &menu,
                self.metrics.css_width as f32,
                self.metrics.css_height,
            );
            if let Some(index) = chrome::history_menu_hit(&layout, menu_x, menu_y) {
                if let Some(menu) = self.history_menu.as_mut() {
                    menu.selected = index;
                }
                self.navigate_selected_history();
            } else if !chrome::contains_rect(layout.rect, menu_x, menu_y) {
                self.history_menu = None;
                self.request_repaint();
            }
            return;
        }
        if self.context_menu.is_some() {
            self.handle_context_menu_click(menu_x, menu_y);
            return;
        }
        if self.modifiers.control_key() && self.open_hovered_link_in_new_tab() {
            return;
        }
        self.click_at_cursor();
    }

    /// Middle click: open the link under the cursor in a new tab.
    fn middle_click(&mut self) {
        let _ = self.open_hovered_link_in_new_tab();
    }

    /// Opens the link under the cursor in a new tab (respecting the tab limit).
    fn open_hovered_link_in_new_tab(&mut self) -> bool {
        let Some((physical_x, physical_y)) = self.cursor else {
            return false;
        };
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let Ok(Some(url)) = crate::links::link_url_at(current.as_ref(), &self.links, x, y) else {
            return false;
        };
        self.save_active_tab();
        match self.tabs.open_tab(url.as_str()) {
            Some(_) => {
                self.load_active_tab();
                true
            }
            None => {
                self.notify("cannot open a new tab: limit reached".to_string());
                false
            }
        }
    }

    /// Opens the right-click context menu for the link/image under the cursor.
    fn open_context_menu(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let (x, y) = self.page_coordinates(physical_x, physical_y);
        let current = self.nav.current().cloned();
        let link = match crate::links::link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => Some(url.as_str()),
            _ => None,
        };
        let image = self.store.frame().and_then(|frame| {
            crate::images::image_at(&frame.images, x, y)
                .filter(|image| image.is_usable())
                .cloned()
        });
        let menu_x = (physical_x / self.metrics.scale) as f32;
        let menu_y = (physical_y / self.metrics.scale) as f32;
        self.context_menu = Some(ContextMenu::new(link, image, menu_x, menu_y));
        self.request_repaint();
    }

    /// Handles a click while the context menu is open (row or click-outside).
    fn handle_context_menu_click(&mut self, x: f32, y: f32) {
        let Some(mut menu) = self.context_menu.clone() else {
            return;
        };
        let layout = chrome::context_menu_layout(
            &menu,
            self.metrics.css_width as f32,
            self.metrics.css_height,
        );
        if let Some(index) = chrome::context_menu_hit(&layout, x, y) {
            menu.selected = index;
            self.context_menu = Some(menu);
            self.execute_selected_menu_action();
        } else if !chrome::contains_rect(layout.rect, x, y) {
            self.context_menu = None;
            self.request_repaint();
        }
    }

    /// Executes the selected context-menu action and closes the menu.
    fn execute_selected_menu_action(&mut self) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let Some(action) = menu.items.get(menu.selected).map(|item| item.action) else {
            return;
        };
        match action {
            MenuAction::OpenLinkInNewTab => {
                if let Some(url) = menu.target_link
                    && let Ok(parsed) = net::Url::parse(&url)
                {
                    self.save_active_tab();
                    if self.tabs.open_tab(parsed.as_str()).is_some() {
                        self.load_active_tab();
                    } else {
                        self.notify("cannot open a new tab: limit reached".to_string());
                    }
                }
            }
            MenuAction::CopyLinkAddress => {
                if let Some(url) = menu.target_link {
                    self.clipboard = url.clone();
                    self.notify(format!(
                        "copied link address to the internal clipboard: {url}"
                    ));
                }
            }
            MenuAction::SaveImage => {
                if let Some(image) = menu.target_image
                    && let Ok(url) = net::Url::parse(&image.url)
                {
                    self.save_image_resource(&url);
                }
            }
            MenuAction::CopyImage => {
                self.notify(
                    "copy image is not supported (internal clipboard is text-only)".to_string(),
                );
            }
            MenuAction::Reload => self.reload(),
            MenuAction::SavePagePng => self.save_frame_png(),
            MenuAction::SavePageHtml => self.save_frame_html(),
        }
        self.request_repaint();
    }

    /// Fetches an image resource on demand and saves it as a download.
    fn save_image_resource(&mut self, url: &net::Url) {
        let mut loader = net::ResourceLoader::new();
        match loader.load(url) {
            Ok(resource) => {
                let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
                match crate::download::save_image(&dir, &resource.url, &resource.bytes) {
                    Ok(saved) => self.notify(saved.notice()),
                    Err(error) => self.notify(format!("cannot save image: {error}")),
                }
            }
            Err(error) => self.notify(format!("cannot fetch image: {error}")),
        }
    }

    fn click_at_cursor(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let scale = self.metrics.scale;
        let chrome_x = (physical_x / scale) as f32;
        let chrome_y = (physical_y / scale) as f32;
        if chrome::in_chrome(chrome_x, chrome_y, self.metrics.css_width as f32) {
            self.chrome_click(chrome_x, chrome_y);
            return;
        }
        let (x, y) = physical_to_page(
            physical_x,
            physical_y,
            self.scale_factor,
            self.scroll.offset(),
        );
        if let Some(menu) = &self.overflow_menu {
            let titles: Vec<String> = self
                .tabs
                .tabs()
                .iter()
                .map(crate::tabs::Tab::display_title)
                .collect();
            let strip = chrome::layout_tabs(
                &titles,
                self.metrics.css_width as f32,
                self.tabs.active_index(),
            );
            let layout = chrome::overflow_menu_layout(
                &strip,
                &titles,
                self.metrics.css_width as f32,
                self.metrics.css_height,
                menu,
            );
            match chrome::overflow_menu_hit(&layout, menu, chrome_x, chrome_y) {
                Some(chrome::OverflowHit::Activate(index)) => {
                    self.overflow_menu = None;
                    self.activate_tab(index);
                    return;
                }
                Some(chrome::OverflowHit::Close(index)) => {
                    self.overflow_menu = None;
                    self.save_active_tab();
                    let _ = self.tabs.close(index);
                    self.load_active_tab();
                    return;
                }
                None => {
                    if !chrome::contains_rect(layout.rect, chrome_x, chrome_y) {
                        self.overflow_menu = None;
                        self.request_repaint();
                    }
                    return;
                }
            }
        }
        if let Some(dropdown) = &self.dropdown {
            let list = chrome::dropdown_layout(
                &dropdown.control_rect,
                dropdown.options.len(),
                self.metrics.css_height,
                self.scroll.offset(),
                dropdown.scroll,
            );
            if let Some(option_index) = list.hit_option(chrome_x, chrome_y) {
                let form_id = dropdown.form_id;
                self.select_option(form_id, option_index);
                return;
            }
            if list.contains_point(chrome_x, chrome_y) {
                return;
            }
            self.dropdown = None;
            self.request_repaint();
            return;
        }
        if let Some(index) = crate::forms::control_at(&self.forms, x, y) {
            self.form_click(index);
            return;
        }
        let current = self.nav.current().cloned();
        match link_url_at(current.as_ref(), &self.links, x, y) {
            Ok(Some(url)) => self.navigate_to(url),
            Ok(None) => {}
            Err(error) => eprintln!("megi: link resolution failed: {error}"),
        }
    }

    /// Debounced resize/scale change: schedule a render after the quiet period.
    fn schedule_resize(&mut self, event_loop: &ActiveEventLoop, metrics: WindowMetrics) {
        self.pending_metrics = Some(metrics);
        let deadline = self.debounce.schedule(Instant::now());
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }

    /// Applies all pending worker messages; returns whether the view changed.
    fn drain_worker_messages(&mut self) -> bool {
        let mut applied = false;
        let mut messages = Vec::new();
        if let Some(receiver) = &self.message_receiver {
            while let Ok(message) = receiver.try_recv() {
                messages.push(message);
            }
        }
        let mut sync_find = false;
        for message in messages {
            if !self.store.accept(message) {
                continue; // stale generation
            }
            applied = true;
            match self.store.frame() {
                Some(frame) => {
                    self.error = None;
                    self.render_pending = false;
                    self.content_capped = frame.capped;
                    self.links = frame.links.clone();
                    self.doc_title = frame.title.clone();
                    if !frame.document_url.starts_with("data:") {
                        let title = frame
                            .title
                            .clone()
                            .unwrap_or_else(|| frame.document_url.clone());
                        let logical = self.recent_history.record(&title, &frame.document_url);
                        self.tabs.active_mut().history_log.record(
                            &title,
                            &frame.document_url,
                            logical,
                        );
                    }
                    self.frame_scripts_executed = frame.scripts_executed;
                    self.frame_timers_pending = frame.script_timers_pending;
                    self.frame_warnings = frame.warnings;
                    self.frame_script_errors = frame.script_errors;
                    self.forms = frame.forms.clone();
                    self.document = Some(Arc::clone(&frame.document));
                    self.find_rects = frame.find_rects.clone();
                    if self.find_editing && !self.find.query().is_empty() {
                        self.find.set_matches(frame.find_matches.clone());
                        sync_find = true;
                    }
                    // A patched-DOM render has a data: document URL: keep the real page URL
                    // in history/`input` (only store redirects for normal renders).
                    let patched_render = frame.document_url.starts_with("data:");
                    if patched_render {
                        // Focus was re-created from the fresh control list; nothing to sync.
                    } else {
                        self.focus = FocusState::new();
                        self.editor = None;
                        self.release_button = None;
                    }
                    self.scroll.set_content_height(frame.content_height);
                    // Auto-refresh only when the page ran scripts and left timers pending;
                    // one-shot: the next frame re-arms it or clears it (no loops otherwise).
                    if self
                        .refresh
                        .should_refresh(frame.scripts_executed, frame.script_timers_pending)
                    {
                        self.refresh.schedule(Instant::now());
                    } else {
                        self.refresh.clear();
                    }
                    // Server redirects: the final URL replaces the requested history entry.
                    if !patched_render
                        && let Ok(final_url) = net::Url::parse(&frame.document_url)
                        && self.nav.current() != Some(&final_url)
                        && self.nav.replace_current(final_url.clone())
                    {
                        self.input = final_url.as_str();
                    }
                }
                None => {
                    self.render_pending = false;
                    self.error = self.store.take_error();
                    // A failed render must not keep auto-refreshing (error loop).
                    self.refresh.clear();
                    if let Some(error) = &self.error {
                        eprintln!("megi: {error}");
                    }
                }
            }
            if sync_find {
                sync_find = false;
                self.scroll_to_current_find();
            }
        }
        if applied {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            // Release a pressed button visual state once its frame is on screen.
            if let Some(form_id) = self.release_button.take() {
                self.apply_form_patch(Some(form_id), None, None, false);
            }
        }
        applied
    }

    /// Scrolls by `delta` CSS pixels and repaints when the offset changed.
    fn scroll_by(&mut self, delta: i64) {
        if self.scroll.scroll_by(delta) {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn scroll_lines(&mut self, lines: i32) {
        if self.scroll.scroll_lines(lines) {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// Updates the window title: document `<title>` + short URL, zoom/scroll percentages,
    /// warning/script-error counters, error text (errors take precedence) and the clipping
    /// marker.
    fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let page = match self.nav.current() {
            Some(url) => title::page_title(self.doc_title.as_deref(), url),
            None => match net::Url::parse(&self.input) {
                Ok(url) => title::page_title(self.doc_title.as_deref(), &url),
                Err(_) => title::FALLBACK_NAME.to_owned(),
            },
        };
        let status = StatusSummary {
            zoom: self.zoom,
            scroll_percent: self.scroll.percent(),
            warnings: self.frame_warnings,
            script_errors: self.frame_script_errors,
        };
        let title = title::status_title(&page, &status, self.error.as_deref(), self.content_capped);
        let title = TabsState::indexed_title(self.tabs.active_index(), self.tabs.len(), &title);
        window.set_title(&title);
    }

    /// Applies a new zoom factor (clamped), recomputes the CSS viewport and re-renders.
    ///
    /// The scroll offset is kept (clamped by the new viewport/content height) so zooming
    /// in/out does not jump to the top of the page.
    fn set_zoom(&mut self, factor: f64) {
        let factor = zoom::sanitize_zoom(factor);
        if (factor - self.zoom).abs() < f64::EPSILON {
            return;
        }
        self.zoom = factor;
        let (width, height) = self
            .window
            .as_ref()
            .map(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((self.requested_width, self.requested_height));
        self.metrics = window_metrics_zoomed(width, height, self.scale_factor, self.zoom);
        self.scroll.set_viewport_height(self.metrics.css_height);
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_render();
    }

    /// Saves the live tabs into the session file (debounced and on exit).
    fn save_session(&mut self) {
        let Some(path) = self.session_path.clone() else {
            return;
        };
        self.save_active_tab();
        let mut session = crate::session::Session::capture(&self.tabs);
        session.recent = self.recent_history.entries().to_vec();
        match crate::session::save(&path, &session) {
            Ok(()) => self.session_dirty = false,
            Err(error) => eprintln!("megi: cannot save session {}: {error}", path.display()),
        }
    }

    /// Sets a transient status notice (downloads/session events) and echoes it to stdout.
    fn notify(&mut self, message: String) {
        println!("megi: {message}");
        self.status = Some(message);
        self.update_title();
        self.request_repaint();
    }

    /// `Ctrl+S`: writes the **render-produced PNG bytes** of the current frame as a
    /// download (no re-encoding, no re-render). The saved image is the whole rendered page
    /// (full content height, not the scrolled viewport; limits documented).
    fn save_frame_png(&mut self) {
        let Some(frame) = self.store.frame() else {
            self.notify("no page to save yet".to_string());
            return;
        };
        let png = frame.png_bytes.clone();
        if png.is_empty() {
            self.notify("cannot save PNG: the render produced no PNG bytes".to_string());
            return;
        }
        let url = match self.nav.current().cloned() {
            Some(url) => url,
            None => match net::Url::parse(&self.input) {
                Ok(url) => url,
                Err(error) => {
                    self.notify(format!("cannot save PNG: invalid URL ({error})"));
                    return;
                }
            },
        };
        let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
        match crate::download::save_png(&dir, &url, &png) {
            Ok(saved) => self.notify(saved.notice()),
            Err(error) => self.notify(format!("cannot save PNG: {error}")),
        }
    }

    /// `Ctrl+Shift+S`: saves the rendered DOM as an HTML download (doctype included).
    fn save_frame_html(&mut self) {
        let Some(document) = self.document.clone() else {
            self.notify("no page to save yet".to_string());
            return;
        };
        let url = match self.nav.current().cloned() {
            Some(url) => url,
            None => match net::Url::parse(&self.input) {
                Ok(url) => url,
                Err(error) => {
                    self.notify(format!("cannot save HTML: invalid URL ({error})"));
                    return;
                }
            },
        };
        let dir = crate::download::resolve_download_dir(self.download_dir.as_deref());
        match crate::download::save_html(&dir, &url, &document) {
            Ok(saved) => self.notify(saved.notice()),
            Err(error) => self.notify(format!("cannot save HTML: {error}")),
        }
    }

    /// Unique URLs from every tab's history, newest first (address-bar autocomplete source).
    fn address_history(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        let mut urls = Vec::new();
        for tab in self.tabs.tabs() {
            for url in tab.navigation.history().iter().rev() {
                let value = url.as_str();
                if seen.insert(value.clone()) {
                    urls.push(value);
                }
            }
        }
        urls
    }

    /// Navigates to the entry selected in the Ctrl+H history menu.
    fn navigate_selected_history(&mut self) {
        let Some(menu) = self.history_menu.take() else {
            return;
        };
        if let Some(entry) = menu.selected_entry()
            && let Ok(url) = net::Url::parse(&entry.url)
        {
            self.start_navigation(&url);
        }
        self.request_repaint();
    }

    /// URL text shown in the address bar (current history entry, else the input).
    fn current_url_text(&self) -> String {
        self.nav.current_url().unwrap_or_else(|| self.input.clone())
    }

    /// Repaints the window without changing the title.
    fn request_repaint(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Opens the address bar for editing (`Ctrl+L`), pre-filled with the current URL.
    fn open_address_bar(&mut self) {
        let url = self.current_url_text();
        self.dropdown = None;
        self.address.open(&url);
        self.request_repaint();
    }

    /// Handles keystrokes while the address bar is edited; returns whether consumed.
    fn handle_address_key(&mut self, key: &Key, ctrl: bool, alt: bool, shift: bool) -> bool {
        if !self.address.is_editing() {
            return false;
        }
        match key {
            Key::Named(NamedKey::Escape) => {
                self.address.cancel();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Enter) => {
                let base = self.nav.current().cloned();
                match chrome::resolve_address(self.address.text(), base.as_ref()) {
                    Ok(url) => {
                        self.address.finish();
                        self.start_navigation(&url);
                    }
                    Err(error) => eprintln!("megi: {error}"),
                }
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Tab) => {
                let history = self.address_history();
                self.address.accept_completion(&history);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Backspace) if ctrl => {
                self.address.delete_word_back();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Backspace) => {
                self.address.backspace();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Delete) => {
                self.address.delete_forward();
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Home) => {
                self.address.move_home(shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::End) => {
                self.address.move_end(shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.address.move_caret(false, shift);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.address.move_caret(true, shift);
                self.request_repaint();
                true
            }
            Key::Character(text) if ctrl => {
                if text.eq_ignore_ascii_case("a") {
                    self.address.select_all();
                } else if text.eq_ignore_ascii_case("c") {
                    self.address.copy();
                } else if text.eq_ignore_ascii_case("x") {
                    self.address.cut();
                } else if text.eq_ignore_ascii_case("v") {
                    self.address.paste();
                } else {
                    return false;
                }
                self.request_repaint();
                true
            }
            Key::Character(text) if !alt => {
                self.address.insert(text);
                self.request_repaint();
                true
            }
            _ => false,
        }
    }

    /// Handles keys while the `<select>` dropdown is open; returns whether consumed.
    fn handle_dropdown_key(&mut self, key: &Key) -> bool {
        let Some(dropdown) = self.dropdown.as_mut() else {
            return false;
        };
        match key {
            Key::Named(NamedKey::Escape) => {
                self.dropdown = None;
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                dropdown.move_active(true);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                dropdown.move_active(false);
                self.request_repaint();
                true
            }
            Key::Named(NamedKey::Enter) => {
                let form_id = dropdown.form_id;
                let active = dropdown.active();
                self.dropdown = None;
                if let Some(option_index) = active {
                    self.select_option(form_id, option_index);
                }
                true
            }
            _ => false,
        }
    }

    /// Applies a `<select>` choice to the DOM snapshot and re-renders it.
    fn select_option(&mut self, form_id: u32, option_index: usize) {
        self.dropdown = None;
        let Some(document) = self.document.clone() else {
            return;
        };
        let Some(sender) = self.request_sender.as_ref() else {
            return;
        };
        let mut patched = (*document).clone();
        if !forms::patch_select(&mut patched, form_id, option_index) {
            return;
        }
        let snapshot = forms::snapshot_data_url(&patched);
        let base = self.nav.current_url();
        let request = self.store.begin_patched_request(
            &snapshot,
            self.metrics.css_width,
            self.metrics.css_height,
            base.as_deref(),
        );
        let _ = sender.send(request);
        self.document = Some(Arc::new(patched));
        self.update_title();
    }

    /// Handles clicks inside the chrome band (tab strip or address bar).
    fn chrome_click(&mut self, x: f32, y: f32) {
        let buttons = chrome::navigation_buttons(self.metrics.css_width as f32);
        if let Some(button) = chrome::navigation_button_at(
            &buttons,
            x,
            y,
            self.nav.can_go_back(),
            self.nav.can_go_forward(),
        ) {
            match button {
                NavigationButton::Back => self.go_back(),
                NavigationButton::Forward => self.go_forward(),
                NavigationButton::Reload => self.reload(),
            }
            return;
        }
        if y >= chrome::TAB_STRIP_HEIGHT {
            self.open_address_bar();
            return;
        }
        let titles: Vec<String> = self
            .tabs
            .tabs()
            .iter()
            .map(crate::tabs::Tab::display_title)
            .collect();
        let layout = chrome::layout_tabs(
            &titles,
            self.metrics.css_width as f32,
            self.tabs.active_index(),
        );
        match chrome::tab_strip_hit(&layout, x, y) {
            Some(TabHit::Activate(index)) => {
                self.overflow_menu = None;
                self.activate_tab(index);
            }
            Some(TabHit::Close(index)) => {
                self.overflow_menu = None;
                self.save_active_tab();
                let _ = self.tabs.close(index);
                self.load_active_tab();
            }
            Some(TabHit::NewTab) => {
                self.overflow_menu = None;
                self.open_new_tab();
            }
            Some(TabHit::Overflow) => {
                let first_hidden = layout.tabs.len();
                let hidden: Vec<usize> = (first_hidden..self.tabs.len()).collect();
                self.overflow_menu = Some(chrome::OverflowMenu::open(hidden));
            }
            None => self.overflow_menu = None,
        }
        self.request_repaint();
    }

    /// Focuses/clicks a form control: text fields open the editor, checkboxes/radios toggle
    /// `checked`, buttons flash their pressed state. Disabled controls are ignored.
    fn form_click(&mut self, index: usize) {
        let Some(control) = self.forms.get(index).cloned() else {
            return;
        };
        if control.disabled || !self.focus.focus(index, self.forms.len()) {
            return;
        }
        if control.kind == FormControlKind::Select {
            self.editor = None;
            self.dropdown = Some(DropdownState::open(
                control.form_id,
                control.rect,
                control.options.clone(),
            ));
            self.request_repaint();
            return;
        }
        if control.kind.is_text_editable() {
            self.editor = Some(TextEdit::new(&control.value));
            self.apply_form_patch(Some(control.form_id), None, None, false);
        } else if control.kind.is_checkable() {
            self.editor = None;
            self.apply_form_patch(Some(control.form_id), None, Some(!control.checked), false);
        } else if control.kind.is_button() {
            self.editor = None;
            self.apply_form_patch(Some(control.form_id), None, None, true);
            self.release_button = Some(control.form_id);
        } else {
            self.editor = None;
            self.apply_form_patch(Some(control.form_id), None, None, false);
        }
    }

    /// Patches the current DOM snapshot (value/checked/focus/active) and re-renders it as an
    /// inline `data:` document with the original page URL as resource base and scripts
    /// disabled (the snapshot already contains their effects).
    fn apply_form_patch(
        &mut self,
        form_id: Option<u32>,
        value: Option<&str>,
        checked: Option<bool>,
        active: bool,
    ) {
        let Some(document) = self.document.clone() else {
            return;
        };
        let Some(sender) = self.request_sender.as_ref() else {
            return;
        };
        let focused = self.focus.focused().is_some();
        let target = form_id.or_else(|| {
            self.focus
                .focused()
                .and_then(|index| self.forms.get(index))
                .map(|control| control.form_id)
        });
        let Some(target) = target else {
            return;
        };
        let mut patched = (*document).clone();
        forms::patch_control(&mut patched, target, value, checked, focused, active);
        let snapshot = forms::snapshot_data_url(&patched);
        let base = self.nav.current_url();
        let request = self.store.begin_patched_request(
            &snapshot,
            self.metrics.css_width,
            self.metrics.css_height,
            base.as_deref(),
        );
        let _ = sender.send(request);
        self.document = Some(Arc::new(patched));
        self.update_title();
    }

    /// Handles typing/Tab/Escape while a form control is focused; returns `true` when the key
    /// was consumed.
    fn handle_form_key(&mut self, key: &Key, ctrl_pressed: bool, alt_pressed: bool) -> bool {
        if matches!(key, Key::Named(NamedKey::Tab)) && !self.forms.is_empty() {
            self.focus.next(self.forms.len());
            self.editor = self
                .focus
                .focused()
                .and_then(|index| self.forms.get(index))
                .filter(|control| control.kind.is_text_editable())
                .map(|control| TextEdit::new(&control.value));
            if let Some(index) = self.focus.focused()
                && let Some(control) = self.forms.get(index).cloned()
            {
                self.apply_form_patch(Some(control.form_id), None, None, false);
            }
            return true;
        }
        if matches!(key, Key::Named(NamedKey::Escape)) && self.focus.focused().is_some() {
            self.focus.blur();
            self.editor = None;
            // Clearing focus from every control: reuse the previously focused id.
            if let Some(control) = self.forms.first().cloned() {
                self.apply_form_patch(Some(control.form_id), None, None, false);
            }
            return true;
        }
        if ctrl_pressed || alt_pressed {
            return false;
        }
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        match key {
            Key::Character(text) => {
                if !editor.insert(text) {
                    return true;
                }
                let value = editor.value().to_owned();
                self.apply_form_patch(None, Some(&value), None, false);
                true
            }
            Key::Named(NamedKey::Backspace) => {
                if editor.backspace() {
                    let value = editor.value().to_owned();
                    self.apply_form_patch(None, Some(&value), None, false);
                }
                true
            }
            Key::Named(NamedKey::Enter) => true, // no backend: Enter never submits
            Key::Named(NamedKey::ArrowLeft) => {
                editor.move_caret(false);
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                editor.move_caret(true);
                true
            }
            _ => false,
        }
    }

    /// Copies the live window state of the active tab back into its [`Tab`] record.
    fn save_active_tab(&mut self) {
        let tab = self.tabs.active_mut();
        tab.url.clone_from(&self.input);
        tab.navigation.clone_from(&self.nav);
        tab.title.clone_from(&self.doc_title);
        tab.scroll = self.scroll;
        tab.zoom = self.zoom;
        tab.find = self.find.clone();
        tab.find_editing = self.find_editing;
        tab.error.clone_from(&self.error);
        tab.content_capped = self.content_capped;
    }

    /// Loads the active tab into the live window state and requests a fresh render.
    ///
    /// The worker's generation counter discards any in-flight frame from the previously
    /// active tab, so frames never leak between tabs.
    fn load_active_tab(&mut self) {
        let tab = self.tabs.active().clone();
        self.input = tab.url;
        self.nav = tab.navigation;
        self.doc_title = tab.title;
        self.zoom = zoom::sanitize_zoom(tab.zoom);
        let (width, height) = self
            .window
            .as_ref()
            .map(|window| {
                let size = window.inner_size();
                (size.width, size.height)
            })
            .unwrap_or((self.requested_width, self.requested_height));
        self.metrics = window_metrics_zoomed(width, height, self.scale_factor, self.zoom);
        self.scroll = tab.scroll;
        self.scroll.set_viewport_height(self.metrics.css_height);
        self.scroll.clamp();
        self.find = tab.find;
        self.find_editing = tab.find_editing;
        self.find_rects.clear();
        self.error = tab.error;
        self.content_capped = tab.content_capped;
        self.forms.clear();
        self.document = None;
        self.links.clear();
        self.frame_scripts_executed = 0;
        self.frame_timers_pending = 0;
        self.frame_warnings = 0;
        self.frame_script_errors = 0;
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        self.request_render();
    }

    /// Opens a blank New Tab (`Ctrl+T`).
    fn open_new_tab(&mut self) {
        self.save_active_tab();
        if self.tabs.open_tab(tabs::BLANK_TAB_URL).is_some() {
            self.load_active_tab();
        }
    }

    /// Closes the active tab (`Ctrl+W`); the last tab is reset to New Tab.
    fn close_active_tab(&mut self) {
        self.save_active_tab();
        match self.tabs.close_active() {
            CloseOutcome::Closed | CloseOutcome::ResetLast => self.load_active_tab(),
            CloseOutcome::Invalid => {}
        }
    }

    /// Activates the next (`forward`) or previous tab, wrapping around.
    fn cycle_tab(&mut self, forward: bool) {
        self.save_active_tab();
        if forward {
            self.tabs.next_tab();
        } else {
            self.tabs.previous_tab();
        }
        self.load_active_tab();
    }

    /// Cycles `delta` tabs forward/backward (mouse wheel over the tab strip).
    fn cycle_tab_by(&mut self, delta: i32) {
        if delta == 0 || self.tabs.len() < 2 {
            return;
        }
        self.save_active_tab();
        let count = self.tabs.len();
        let current = self.tabs.active_index() as i32;
        let target = (current + delta).rem_euclid(count as i32) as usize;
        let _ = self.tabs.activate(target);
        self.load_active_tab();
    }

    /// Activates tab `index` (0-based) if it exists.
    fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.tabs.active_index() {
            return;
        }
        self.save_active_tab();
        if self.tabs.activate(index) {
            self.load_active_tab();
        }
    }

    /// Opens the find prompt (Ctrl+F): next keystrokes edit the query.
    fn open_find(&mut self) {
        self.find_editing = true;
        self.find.clear();
        self.find_rects.clear();
        self.editor = None;
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Closes find-in-page, clearing the query/highlights and re-rendering without hits.
    fn close_find(&mut self) {
        if !self.find_editing && self.find.query().is_empty() {
            return;
        }
        self.find_editing = false;
        self.find.clear();
        self.find_rects.clear();
        if self.request_sender.is_some() {
            self.request_render();
        }
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Re-renders with the current query so the worker computes hits/highlight rectangles.
    fn refresh_find_results(&mut self) {
        if self.find.query().is_empty() {
            self.find_rects.clear();
        }
        self.request_render();
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Scrolls so the current hit is visible (centered when possible).
    fn scroll_to_current_find(&mut self) {
        if self.find_rects.is_empty() {
            return;
        }
        let rect = self.find_rects[self.find.current() % self.find_rects.len()];
        let offset =
            find::scroll_offset_for(&rect, self.metrics.css_height, self.scroll.max_offset());
        if self.scroll.scroll_to(offset) {
            self.update_title();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Handles keys while the find prompt is open; returns `true` when consumed.
    fn handle_find_key(&mut self, key: &Key, shift: bool, ctrl: bool, alt: bool) -> bool {
        if !self.find_editing {
            return false;
        }
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.close_find();
            return true;
        }
        if ctrl || alt {
            return false;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                if shift {
                    self.find.previous_hit();
                } else {
                    self.find.next_hit();
                }
                self.scroll_to_current_find();
                self.refresh_find_results_highlights_only();
                true
            }
            Key::Named(NamedKey::Backspace) => {
                let mut query = self.find.query().to_owned();
                query.pop();
                self.find.set_results(query.clone(), Vec::new());
                self.refresh_find_results();
                true
            }
            Key::Character(text) => {
                let mut query = self.find.query().to_owned();
                query.push_str(text);
                self.find.set_results(query, Vec::new());
                self.refresh_find_results();
                true
            }
            _ => false,
        }
    }

    /// Redraws the overlay after moving to another hit (no re-render needed: the frame
    /// already carries the highlight rectangles for the query).
    fn refresh_find_results_highlights_only(&mut self) {
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Prints the final URL and document title (`Ctrl+D`, no address bar yet).
    fn print_document_info(&self) {
        let url = self.nav.current_url().unwrap_or_else(|| self.input.clone());
        let title = match self.nav.current() {
            Some(url) => title::page_title(self.doc_title.as_deref(), url),
            None => title::FALLBACK_NAME.to_owned(),
        };
        println!("url: {url}");
        println!("title: {title}");
    }

    /// Navigates to the first history entry (home) — `Alt+Home`.
    fn go_home(&mut self) {
        if let Some(home) = self.nav.home_target().cloned() {
            self.navigate_to(home);
        } else {
            self.reload();
        }
    }

    /// Handles the zoom shortcuts; returns `true` when the key was consumed.
    fn handle_zoom_key(&mut self, key: &Key) -> bool {
        let Key::Character(text) = key else {
            return false;
        };
        let text = text.as_str();
        if text == "=" || text == "+" {
            let next = zoom::zoom_in(self.zoom);
            self.set_zoom(next);
            true
        } else if text == "-" || text == "_" {
            let next = zoom::zoom_out(self.zoom);
            self.set_zoom(next);
            true
        } else if text == "0" {
            self.set_zoom(zoom::zoom_reset());
            true
        } else {
            false
        }
    }

    /// Copies the visible region of the current frame into the surface.
    fn present(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        // Chrome paint inputs are captured before borrowing the surface mutably.
        let chrome_titles: Vec<String> = self
            .tabs
            .tabs()
            .iter()
            .map(crate::tabs::Tab::display_title)
            .collect();
        let chrome_fallback = self.current_url_text();
        let chrome_address = self.address.clone();
        let chrome_dropdown = self.dropdown.clone();
        let chrome_active_tab = self.tabs.active_index();
        let chrome_scroll = self.scroll.offset();
        let chrome_css_height = self.metrics.css_height;
        let chrome_css_width = self.metrics.css_width as f32;
        let chrome_scale = self.metrics.scale as f32;
        let chrome_history = self.address_history();
        let chrome_completion = self.address.autocomplete(&chrome_history);
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let (Some(width_nonzero), Some(height_nonzero)) =
            (NonZeroU32::new(width), NonZeroU32::new(height))
        else {
            return;
        };
        if surface.resize(width_nonzero, height_nonzero).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let expected = expected_buffer_len(width, height).unwrap_or(0);
        if buffer.len() != expected {
            return;
        }
        // An error always shows a white frame (the previous frame is not reused).
        let frame = if self.error.is_some() {
            None
        } else {
            self.store
                .frame()
                .map(|frame| (frame.pixels.as_slice(), frame.width, frame.height))
        };
        let presented = present::present_region(
            frame,
            self.scroll.offset(),
            self.metrics.css_height,
            &mut buffer,
            width,
            height,
        );
        if !presented {
            buffer.fill(WHITE_0RGB);
        } else if self.find_editing && !self.find_rects.is_empty() {
            let _ = find::overlay_highlights_dest(
                &mut buffer,
                width,
                height,
                self.metrics.css_width,
                self.metrics.css_height,
                self.scroll.offset(),
                &self.find_rects,
                self.find.current(),
            );
        }
        // Browser chrome is an overlay in CSS coordinates over the page frame.
        let favicons: Vec<Option<crate::favicon::Favicon>> = chrome_titles
            .iter()
            .map(|title| Some(crate::favicon::Favicon::fallback(title)))
            .collect();
        let view = ChromeView {
            favicons: &favicons,
            tabs: &chrome_titles,
            active_tab: chrome_active_tab,
            address: &chrome_address,
            fallback_url: &chrome_fallback,
            dropdown: chrome_dropdown.as_ref(),
            scroll_y: chrome_scroll,
            css_viewport_height: chrome_css_height,
            css_width: chrome_css_width,
            completion: chrome_completion.as_deref(),
            loading: self.render_pending,
            overflow: self.overflow_menu.as_ref(),
            hover_url: self.hover_url.as_deref(),
            menu: self.context_menu.as_ref(),
            can_go_back: self.nav.can_go_back(),
            can_go_forward: self.nav.can_go_forward(),
            history_menu: self.history_menu.as_ref(),
        };
        let _ = chrome::draw_chrome(&mut buffer, width, height, chrome_scale, &view);
        let _ = buffer.present();
    }
}

impl ApplicationHandler<UserEvent> for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.ensure_window(event_loop) {
            eprintln!("megi: {error}");
            event_loop.exit();
            return;
        }
        self.present();
        self.request_render();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: UserEvent) {
        self.drain_worker_messages();
        if let Some(deadline) = self.refresh.deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.present(),
            WindowEvent::Resized(size) => {
                let metrics =
                    window_metrics_zoomed(size.width, size.height, self.scale_factor, self.zoom);
                self.schedule_resize(event_loop, metrics);
                self.present();
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                self.scale_factor = scale_factor;
                let (width, height) = self
                    .window
                    .as_ref()
                    .map(|window| {
                        let size = window.inner_size();
                        (size.width, size.height)
                    })
                    .unwrap_or((self.requested_width, self.requested_height));
                let metrics = window_metrics_zoomed(width, height, scale_factor, self.zoom);
                self.schedule_resize(event_loop, metrics);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines.round() as i32,
                    MouseScrollDelta::PixelDelta(position) => (position.y / 40.0).round() as i32,
                };
                // Mouse wheel over the tab strip cycles tabs.
                if let Some((_physical_x, physical_y)) = self.cursor {
                    let scale = self.metrics.scale;
                    let y_css = (physical_y / scale) as f32;
                    if y_css < chrome::TAB_STRIP_HEIGHT {
                        if rows != 0 {
                            self.cycle_tab_by(if rows > 0 { 1 } else { -1 });
                        }
                        return;
                    }
                }
                if let Some(dropdown) = self.dropdown.as_mut()
                    && let Some((physical_x, physical_y)) = self.cursor
                {
                    let scale = self.metrics.scale;
                    let list = chrome::dropdown_layout(
                        &dropdown.control_rect,
                        dropdown.options.len(),
                        self.metrics.css_height,
                        self.scroll.offset(),
                        dropdown.scroll,
                    );
                    if list.contains_point((physical_x / scale) as f32, (physical_y / scale) as f32)
                    {
                        if dropdown.scroll_by(-rows) {
                            self.request_repaint();
                        }
                        return;
                    }
                }
                match delta {
                    MouseScrollDelta::LineDelta(_, lines) => {
                        self.scroll_by(scroll::wheel_line_delta(f64::from(lines)));
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        self.scroll_by(scroll::wheel_pixel_delta(position.y));
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                self.update_hover(position.x, position.y);
                if let Some(dropdown) = self.dropdown.as_mut() {
                    let scale = self.metrics.scale;
                    let list = chrome::dropdown_layout(
                        &dropdown.control_rect,
                        dropdown.options.len(),
                        self.metrics.css_height,
                        self.scroll.offset(),
                        dropdown.scroll,
                    );
                    if let Some(index) =
                        list.hit_row((position.x / scale) as f32, (position.y / scale) as f32)
                        && dropdown.active != Some(index)
                    {
                        dropdown.active = Some(index);
                        self.request_repaint();
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.left_click();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                self.open_context_menu();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Middle,
                ..
            } => {
                self.middle_click();
            }
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let alt_pressed = self.modifiers.alt_key();
                let ctrl_pressed = self.modifiers.control_key();
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("h"))
                {
                    if self.history_menu.is_some() {
                        self.history_menu = None;
                    } else {
                        let rect = chrome::address_bar_rect(self.metrics.css_width as f32);
                        self.history_menu = Some(HistoryMenu::open(
                            self.recent_history.menu_entries(),
                            rect.x,
                            rect.y + rect.height,
                        ));
                    }
                    self.request_repaint();
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("s"))
                {
                    if self.modifiers.shift_key() {
                        self.save_frame_html();
                    } else {
                        self.save_frame_png();
                    }
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("l"))
                {
                    self.open_address_bar();
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("f"))
                {
                    self.open_find();
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("d"))
                {
                    self.print_document_info();
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("t"))
                {
                    self.open_new_tab();
                    return;
                }
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("w"))
                {
                    self.close_active_tab();
                    return;
                }
                if ctrl_pressed && matches!(event.logical_key.as_ref(), Key::Named(NamedKey::Tab)) {
                    self.cycle_tab(!self.modifiers.shift_key());
                    return;
                }
                if alt_pressed
                    && let Key::Character(text) = event.logical_key.as_ref()
                    && let Some(index) = text
                        .chars()
                        .next()
                        .and_then(|digit| digit.to_digit(10))
                        .filter(|digit| (1..=9).contains(digit))
                {
                    self.activate_tab(index as usize - 1);
                    return;
                }
                if self.history_menu.is_some() {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.history_menu = None;
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(menu) = self.history_menu.as_mut() {
                                menu.move_selection(true);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(menu) = self.history_menu.as_mut() {
                                menu.move_selection(false);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.navigate_selected_history();
                            return;
                        }
                        _ => {}
                    }
                }
                if self.context_menu.is_some() {
                    match event.logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            self.context_menu = None;
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(menu) = self.context_menu.as_mut() {
                                menu.move_selection(true);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(menu) = self.context_menu.as_mut() {
                                menu.move_selection(false);
                            }
                            self.request_repaint();
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.execute_selected_menu_action();
                            return;
                        }
                        _ => {}
                    }
                }
                if self.overflow_menu.is_some()
                    && matches!(event.logical_key.as_ref(), Key::Named(NamedKey::Escape))
                {
                    self.overflow_menu = None;
                    self.request_repaint();
                    return;
                }
                if self.handle_dropdown_key(&event.logical_key) {
                    return;
                }
                if self.handle_address_key(
                    &event.logical_key,
                    ctrl_pressed,
                    alt_pressed,
                    self.modifiers.shift_key(),
                ) {
                    return;
                }
                if self.handle_find_key(
                    &event.logical_key,
                    self.modifiers.shift_key(),
                    ctrl_pressed,
                    alt_pressed,
                ) {
                    return;
                }
                if (ctrl_pressed || self.editor.is_none())
                    && self.handle_zoom_key(&event.logical_key)
                {
                    return;
                }
                if self.handle_form_key(&event.logical_key, ctrl_pressed, alt_pressed) {
                    return;
                }
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::ArrowUp) => self.scroll_lines(-1),
                    Key::Named(NamedKey::ArrowDown) => self.scroll_lines(1),
                    Key::Named(NamedKey::PageUp) => {
                        if self.scroll.page_up() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::PageDown) => {
                        if self.scroll.page_down() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::F5) => self.reload(),
                    Key::Named(NamedKey::Home) if alt_pressed => self.go_home(),
                    Key::Named(NamedKey::Home) => {
                        if self.scroll.home() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::End) => {
                        if self.scroll.end() {
                            self.update_title();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::Backspace) => self.go_back(),
                    Key::Named(NamedKey::ArrowLeft) if alt_pressed => self.go_back(),
                    Key::Named(NamedKey::ArrowRight) if alt_pressed => self.go_forward(),
                    Key::Character(text) if text.eq_ignore_ascii_case("q") => {
                        event_loop.exit();
                    }
                    Key::Character(text) if text.eq_ignore_ascii_case("r") => self.reload(),
                    Key::Character(text) if text.eq_ignore_ascii_case("g") => self.reload(),
                    Key::Character(text) if text.eq_ignore_ascii_case("b") => self.go_back(),
                    Key::Character(text) if text.eq_ignore_ascii_case("n") => {
                        self.go_forward();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        // Auto-refresh: re-render the current URL (same generation-based worker path, scroll
        // preserved). The new frame re-arms the timer only while timers remain pending.
        if self.refresh.take_due(now) {
            self.request_render();
        }
        if self.debounce.take_due(now)
            && let Some(metrics) = self.pending_metrics.take()
        {
            self.metrics = metrics;
            self.scroll.set_viewport_height(metrics.css_height);
            self.request_render();
        }
        if self.session_dirty && self.session_debounce.take_due(now) {
            self.save_session();
        }
        let mut deadline = match (self.debounce.deadline(), self.refresh.deadline()) {
            (Some(debounce), Some(refresh)) => Some(debounce.min(refresh)),
            (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
            (None, None) => None,
        };
        if self.session_dirty
            && let Some(session_deadline) = self.session_debounce.deadline()
        {
            deadline =
                Some(deadline.map_or(session_deadline, |deadline| deadline.min(session_deadline)));
        }
        match deadline {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Persist the session before shutting the worker down.
        self.save_session();
        // Dropping the sender stops the worker; join so the process exits cleanly.
        self.request_sender = None;
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}
