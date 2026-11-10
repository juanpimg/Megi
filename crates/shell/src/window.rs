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
//! - **dynamic title/status**: the window title shows the document `<title>` plus the short
//!   URL, `zoom%`, `scroll%`, warning/script-error counters ([`crate::title`]); `Ctrl+L`
//!   prints the current URL to stdout (no address bar yet);
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
//! | `Ctrl+L` | Print the current URL to stdout |
//! | `Ctrl+F` | Find in page (type, `Enter` next, `Shift+Enter` previous, `Esc` close) |
//! | `Ctrl+D` | Print the final URL and page title |
//! | `F5`, `g` | Reload |
//! | `Alt+Home` | Go to the first (home) history entry |
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

use crate::convert::{WHITE_0RGB, expected_buffer_len};
use crate::error::ShellError;
use crate::find::{self, FindState};
use crate::forms::{self, FocusState, FormControl, TextEdit};
use crate::links::{LinkRegion, link_url_at, physical_to_page};
use crate::navigation::{NavigationOutcome, NavigationState};
use crate::present;
use crate::refresh::{RefreshPolicy, RefreshTimer};
use crate::scale::{WindowMetrics, window_metrics_zoomed};
use crate::scroll::{self, ScrollState};
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowViewOptions {
    /// Initial page zoom factor (clamped to 50–300 %).
    pub zoom: f64,
    /// Auto-refresh policy for pages with pending timers.
    pub refresh: RefreshPolicy,
}

impl Default for WindowViewOptions {
    fn default() -> Self {
        Self {
            zoom: zoom::DEFAULT_ZOOM,
            refresh: RefreshPolicy::disabled(),
        }
    }
}

impl WindowViewOptions {
    /// View options with an explicit initial zoom and refresh policy.
    pub const fn new(zoom: f64, refresh: RefreshPolicy) -> Self {
        Self { zoom, refresh }
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
    /// Highlight rectangles of the last frame (page CSS coordinates).
    find_rects: Vec<crate::links::CssRect>,
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
        let zoom = zoom::sanitize_zoom(view.zoom);
        let mut nav = NavigationState::new();
        if let Ok(url) = net::Url::parse(&input) {
            nav.navigate_to(url);
        }
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
            scroll: ScrollState::default(),
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
        let request = if self.find.query().is_empty() {
            self.store
                .begin_request(&self.input, self.metrics.css_width, self.metrics.css_height)
        } else {
            self.store.begin_find_request(
                &self.input,
                self.metrics.css_width,
                self.metrics.css_height,
                self.find.query(),
            )
        };
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
    fn click_at_cursor(&mut self) {
        let Some((physical_x, physical_y)) = self.cursor else {
            return;
        };
        let (x, y) = physical_to_page(
            physical_x,
            physical_y,
            self.scale_factor,
            self.scroll.offset(),
        );
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
                    self.content_capped = frame.capped;
                    self.links = frame.links.clone();
                    self.doc_title = frame.title.clone();
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

    /// Prints the current URL to stdout (`Ctrl+L`; there is no address bar yet).
    fn print_current_url(&self) {
        if let Some(url) = self.nav.current_url() {
            println!("{url}");
        } else {
            println!("{}", self.input);
        }
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
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, lines) => {
                    self.scroll_by(scroll::wheel_line_delta(f64::from(lines)));
                }
                MouseScrollDelta::PixelDelta(position) => {
                    self.scroll_by(scroll::wheel_pixel_delta(position.y));
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.click_at_cursor();
            }
            // Right/middle clicks are deliberately ignored (no context menu yet).
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let alt_pressed = self.modifiers.alt_key();
                let ctrl_pressed = self.modifiers.control_key();
                if ctrl_pressed
                    && matches!(event.logical_key.as_ref(), Key::Character(text) if text.eq_ignore_ascii_case("l"))
                {
                    self.print_current_url();
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
        let deadline = match (self.debounce.deadline(), self.refresh.deadline()) {
            (Some(debounce), Some(refresh)) => Some(debounce.min(refresh)),
            (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
            (None, None) => None,
        };
        match deadline {
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Dropping the sender stops the worker; join so the process exits cleanly.
        self.request_sender = None;
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}
