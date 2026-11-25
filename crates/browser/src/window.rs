//! Real winit window with a softbuffer CPU framebuffer.
//!
//! **This module creates an `EventLoop` and must never run in tests.** The unit
//! and integration tests exercise the pure logic in [`crate::cli`],
//! [`crate::convert`], [`crate::scroll`], [`crate::scale`], [`crate::present`]
//! and [`crate::worker`]; CI is headless (`--screenshot`) and never opens a
//! window.
//!
//! When built without the `window` feature (`--no-default-features`), window
//! entry points remain callable and return a typed [`ShellError::Window`] error
//! instead of linking `winit` or `softbuffer`.

use crate::error::ShellError;
use crate::refresh::RefreshPolicy;
use crate::zoom;

#[cfg(all(feature = "window", feature = "a11y"))]
mod accessibility;
#[cfg(feature = "window")]
use settings::chrome_mode;
#[cfg(feature = "window")]
mod desktop;
#[cfg(feature = "window")]
mod downloads;
#[cfg(feature = "window")]
mod events;
#[cfg(feature = "window")]
mod form_controls;
#[cfg(feature = "window")]
mod frontend;
#[cfg(feature = "window")]
mod input;
#[cfg(feature = "window")]
mod keyboard;
#[cfg(feature = "window")]
mod presentation;
/// Pure rail geometry and state helpers, usable without a windowing backend.
pub use settings::rail_state;
#[cfg(feature = "window")]
mod save_as;
mod settings;
#[cfg(feature = "window")]
#[cfg(feature = "window")]
mod shortcuts;
#[cfg(all(test, feature = "window"))]
mod tests;
#[cfg(feature = "window")]
mod ui;
#[cfg(feature = "window")]
mod worker;

#[cfg(feature = "window")]
use desktop::*;

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
    /// Suppress chrome animation (`prefers-reduced-motion: reduce`, opt-in flag).
    pub reduced_motion: bool,
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
            reduced_motion: false,
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
            reduced_motion: false,
        }
    }

    /// Enables OS renderer processes with DOM-patching IPC (ADR-004).
    pub fn with_process_render(mut self, enabled: bool) -> Self {
        self.process_render = enabled;
        self
    }

    /// Suppresses chrome transitions when the user prefers reduced motion.
    pub fn with_reduced_motion(mut self, enabled: bool) -> Self {
        self.reduced_motion = enabled;
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
/// `$DISPLAY` / `$WAYLAND_DISPLAY`), when built without the `window` feature,
/// or when winit/softbuffer initialization fails.
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
    #[cfg(feature = "window")]
    {
        desktop::run(input, title, width, height, render_options, view)
    }
    #[cfg(not(feature = "window"))]
    {
        let _ = (input, title, width, height, render_options, view);
        Err(ShellError::Window(
            "window mode requires the `window` feature (rebuild with default features or `--features window`, or pass `--screenshot PATH` for headless mode)"
                .to_owned(),
        ))
    }
}
