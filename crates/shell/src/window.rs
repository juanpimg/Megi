//! Real winit window with a softbuffer CPU framebuffer.
//!
//! **This module creates an `EventLoop` and must never run in tests.** The unit and
//! integration tests exercise the pure logic in [`crate::cli`], [`crate::convert`] and
//! [`crate::headless`]; CI is headless (`--screenshot`) and does not open a window.
//!
//! Behaviour:
//! - one window, one render at the current inner size (physical pixels);
//! - resize triggers a re-render after a 150 ms debounce so a drag does not queue dozens
//!   of expensive renders;
//! - `q`/`Esc`/close button quit, `r` re-renders; errors are logged to stderr and the
//!   window shows an opaque-white frame with a `Megi — error` title.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::convert::{WHITE_0RGB, expected_buffer_len, pixmap_to_0rgb};
use crate::error::ShellError;
use crate::headless;

/// Delay after the last resize event before re-rendering.
pub const RESIZE_DEBOUNCE: Duration = Duration::from_millis(150);

type ShellContext = softbuffer::Context<Arc<Window>>;
type ShellSurface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

/// Runs the interactive window loop until the user closes it.
///
/// Returns [`ShellError::Window`] when no display server is available (no `$DISPLAY` /
/// `$WAYLAND_DISPLAY`) or when winit/softbuffer initialization fails.
pub fn run_window(input: String, title: String, width: u32, height: u32) -> Result<(), ShellError> {
    let event_loop = EventLoop::new().map_err(|error| {
        ShellError::Window(format!(
            "cannot create the event loop (is an X11/Wayland session available?): {error}"
        ))
    })?;
    let mut app = WindowApp {
        input,
        title,
        requested_width: width,
        requested_height: height,
        window: None,
        context: None,
        surface: None,
        pixmap: None,
        pending_resize: None,
        resize_deadline: None,
    };
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
    pixmap: Option<paint::Pixmap>,
    pending_resize: Option<(u32, u32)>,
    resize_deadline: Option<Instant>,
}

impl WindowApp {
    /// Creates the window + softbuffer surface on first resume.
    fn ensure_surface(&mut self, event_loop: &ActiveEventLoop) -> Result<(), ShellError> {
        if self.surface.is_some() {
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
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
        Ok(())
    }

    /// Current window inner size in physical pixels (fallback: requested size).
    fn current_size(&self) -> (u32, u32) {
        match &self.window {
            Some(window) => {
                let size = window.inner_size();
                (size.width.max(1), size.height.max(1))
            }
            None => (self.requested_width, self.requested_height),
        }
    }

    /// Renders the document at the current size and presents it.
    fn render_current(&mut self) {
        let (width, height) = self.current_size();
        match headless::render_target(&self.input, width, height) {
            Ok(output) => self.pixmap = Some(output.pixmap),
            Err(error) => {
                eprintln!("megi: {error}");
                self.pixmap = None;
                if let Some(window) = &self.window {
                    window.set_title(&format!("Megi — error: {error}"));
                }
            }
        }
        self.present();
    }

    /// Copies the current pixmap (or a white frame) into the surface and presents it.
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
        let frame = self
            .pixmap
            .as_ref()
            .map(pixmap_to_0rgb)
            .filter(|frame| frame.len() == expected);
        match frame {
            Some(frame) if buffer.len() == frame.len() => buffer.copy_from_slice(&frame),
            _ => buffer.fill(WHITE_0RGB),
        }
        let _ = buffer.present();
    }
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(error) = self.ensure_surface(event_loop) {
            eprintln!("megi: {error}");
            event_loop.exit();
            return;
        }
        self.render_current();
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
                self.pending_resize = Some((size.width.max(1), size.height.max(1)));
                let deadline = Instant::now() + RESIZE_DEBOUNCE;
                self.resize_deadline = Some(deadline);
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character(text) if text.eq_ignore_ascii_case("q") => {
                        event_loop.exit();
                    }
                    Key::Character(text) if text.eq_ignore_ascii_case("r") => {
                        self.render_current();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(deadline) = self.resize_deadline else {
            return;
        };
        if Instant::now() < deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return;
        }
        self.resize_deadline = None;
        if let Some((width, height)) = self.pending_resize.take() {
            self.requested_width = width;
            self.requested_height = height;
        }
        self.render_current();
    }
}
