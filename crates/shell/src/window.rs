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
//!   arrows/PageUp/PageDown/Home/End); `q`/`Esc` quit, `r` forces a re-render,
//!   left click prints the current URL to stdout;
//! - errors produce a white frame plus stderr/title feedback; the window stays
//!   usable.
//!
//! Controls (window mode):
//!
//! | Input | Action |
//! |---|---|
//! | Wheel / touchpad | Scroll vertically |
//! | `↑` / `↓` | Scroll one line |
//! | `PageUp` / `PageDown` | Scroll one page |
//! | `Home` / `End` | Top / bottom |
//! | `r` | Re-render the page |
//! | `q`, `Esc`, close button | Quit |
//! | Left click | Print the current URL to stdout |

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::convert::{WHITE_0RGB, expected_buffer_len};
use crate::error::ShellError;
use crate::present;
use crate::scale::{WindowMetrics, window_metrics};
use crate::scroll::{self, ScrollState};
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
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|error| {
            ShellError::Window(format!(
                "cannot create the event loop (is an X11/Wayland session available?): {error}"
            ))
        })?;
    let proxy = event_loop.create_proxy();
    let mut app = WindowApp::new(input, title, width, height, proxy);
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
}

impl WindowApp {
    fn new(
        input: String,
        title: String,
        width: u32,
        height: u32,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        Self {
            input,
            title,
            requested_width: width,
            requested_height: height,
            window: None,
            context: None,
            surface: None,
            proxy,
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
        let (sender, receiver, handle) = spawn_worker(crate::worker::render_document, move || {
            // Wake the event loop; ignore a closed loop (window closing).
            let _ = proxy.send_event(UserEvent::WorkerReady);
        });
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
                    self.scroll.set_content_height(frame.content_height);
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

    /// Updates the window title with the scroll percentage / error / cap warning.
    fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let title = if let Some(error) = &self.error {
            format!("Megi — error: {error}")
        } else {
            let mut title = format!(
                "{} — {}% ({}/{})",
                self.title,
                self.scroll.percent(),
                self.scroll.offset(),
                self.scroll.max_offset()
            );
            if self.content_capped {
                title.push_str(" [recortado]");
            }
            title
        };
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                // No DOM hit-testing yet: clicking copies the current URL to stdout.
                println!("{}", self.input);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
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
                    Key::Character(text) if text.eq_ignore_ascii_case("q") => {
                        event_loop.exit();
                    }
                    Key::Character(text) if text.eq_ignore_ascii_case("r") => {
                        self.request_render();
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
