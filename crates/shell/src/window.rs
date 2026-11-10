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
//! - **dynamic title**: the window title shows the document `<title>` plus the short URL
//!   and scroll percentage ([`crate::title`]);
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
use crate::links::{LinkRegion, link_url_at, physical_to_page};
use crate::navigation::{NavigationOutcome, NavigationState};
use crate::present;
use crate::scale::{WindowMetrics, window_metrics};
use crate::scroll::{self, ScrollState};
use crate::title;
use crate::worker::{
    Debounce, FrameStore, RENDER_DEBOUNCE, RenderRequest, WorkerMessage, spawn_worker,
};

type ShellContext = softbuffer::Context<Arc<Window>>;
type ShellSurface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

/// Wake-up event sent by the render worker when it has a message.
#[derive(Clone, Copy, Debug)]
enum UserEvent {
    /// A worker message (frame or error) is ready on the channel.
    WorkerReady,
}

/// Runs the interactive window loop until the user closes it.
///
/// Returns [`ShellError::Window`] when no display server is available (no
/// `$DISPLAY` / `$WAYLAND_DISPLAY`) or when winit/softbuffer initialization
/// fails.
pub fn run_window(input: String, title: String, width: u32, height: u32) -> Result<(), ShellError> {
    run_window_with_options(
        input,
        title,
        width,
        height,
        render::RenderOptions::default(),
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
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|error| {
            ShellError::Window(format!(
                "cannot create the event loop (is an X11/Wayland session available?): {error}"
            ))
        })?;
    let proxy = event_loop.create_proxy();
    let mut app = WindowApp::new(input, title, width, height, proxy, render_options);
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
    /// Current keyboard modifiers (for Alt+←/→).
    modifiers: ModifiersState,
}

impl WindowApp {
    fn new(
        input: String,
        title: String,
        width: u32,
        height: u32,
        proxy: EventLoopProxy<UserEvent>,
        render_options: render::RenderOptions,
    ) -> Self {
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
            metrics: window_metrics(width, height, 1.0),
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
        self.metrics = window_metrics(size.width, size.height, self.scale_factor);
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
        let request =
            self.store
                .begin_request(&self.input, self.metrics.css_width, self.metrics.css_height);
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
        self.doc_title = None;
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
                    self.scroll.set_content_height(frame.content_height);
                    // Server redirects: the final URL replaces the requested history entry.
                    if let Ok(final_url) = net::Url::parse(&frame.document_url)
                        && self.nav.current() != Some(&final_url)
                        && self.nav.replace_current(final_url.clone())
                    {
                        self.input = final_url.as_str();
                    }
                }
                None => {
                    self.error = self.store.take_error();
                    if let Some(error) = &self.error {
                        eprintln!("megi: {error}");
                    }
                }
            }
        }
        if applied {
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
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

    /// Updates the window title: document `<title>` + short URL, scroll percentage, error
    /// text (errors take precedence) and the clipping marker.
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
        let title = title::window_title(
            &page,
            self.scroll.percent(),
            self.error.as_deref(),
            self.content_capped,
        );
        window.set_title(&title);
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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: UserEvent) {
        self.drain_worker_messages();
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
                let metrics = window_metrics(size.width, size.height, self.scale_factor);
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
                let metrics = window_metrics(width, height, scale_factor);
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
        if self.debounce.take_due(Instant::now())
            && let Some(metrics) = self.pending_metrics.take()
        {
            self.metrics = metrics;
            self.scroll.set_viewport_height(metrics.css_height);
            self.request_render();
        }
        match self.debounce.deadline() {
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
