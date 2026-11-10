//! # Megi Shell (`shell`) — Phase 7 desktop shell
//!
//! First real Megi browser window: [`winit`] event loop + CPU framebuffer presented with
//! [`softbuffer`] (no GPU, no external compositor APIs), plus a **headless**
//! `--screenshot` mode that runs the same render pipeline and writes a PNG (CI-friendly,
//! no display server required).
//!
//! ## Dependency policy
//!
//! `winit` and `softbuffer` are the low-level window/presentation dependencies explicitly
//! authorized by `docs/ARCHITECTURE.md` §3.1 for the OS shell. Both are built with the
//! Linux **X11** and **Wayland** backends enabled (dynamic loading, so the binary also
//! starts on systems that only have one of the two). No GPU/windowing logic is
//! reimplemented here; the browser pipeline itself (`render` → `net`/`html`/`css`/`layout`/
//! `paint`) stays dependency-free.
//!
//! ## System requirements (window mode)
//!
//! - Linux with an X11 or Wayland session (`$DISPLAY` or `$WAYLAND_DISPLAY` set).
//! - Wayland additionally needs the usual `libwayland`/`libxkbcommon` runtime libraries
//!   provided by the desktop; X11 uses `libX11`/`libxcb` (dlopen).
//! - No display server => use `--screenshot` (headless); window creation would fail with
//!   a typed [`ShellError::Window`] error, and the CLI never creates an `EventLoop` in
//!   headless mode.
//!
//! ## What the window does today
//!
//! - Renders the URL/first file once at the window's inner size and presents the
//!   `paint::Pixmap` as a `0RGB` framebuffer (softbuffer format; alpha is ignored because
//!   the render canvas is opaque white).
//! - Re-renders on resize with a **150 ms debounce** (multiple resize events coalesce into
//!   one expensive render); presents the existing frame on expose/redraw.
//! - Keys: `q`/`Esc`/close button quit, `r` re-renders. Errors are printed to stderr and
//!   the window keeps a white frame with a `Megi — error` title.
//! - Renders happen synchronously in the event handler (the pipeline is blocking for now);
//!   a future WP moves rendering to a worker thread with cancellation.
//!
//! ## What it does not do yet
//!
//! No hit-testing, mouse input, scrolling, navigation history, tabs, zoom, JS or forms.
//! HiDPI: the render viewport is the window's **physical** pixel size (CSS px == device
//! px, scale factor 1) until the layout lane adds device-scale support.
//!
//! ## Headless mode
//!
//! `megi <url|file> --screenshot out.png` never touches winit: it calls
//! [`render::render_url`] with the requested viewport and writes a deterministic PNG. The
//! pure logic (argument parsing, URL normalization, viewport, pixmap→framebuffer
//! conversion, screenshot writing) is unit-tested without a display server.

#![forbid(unsafe_code)]

pub mod cli;
pub mod convert;
pub mod error;
pub mod headless;
pub mod window;

pub use cli::{CliArgs, LaunchMode, finish_args, parse_args, parse_cli};
pub use convert::{PIXEL_FORMAT, expected_buffer_len, pixmap_to_0rgb};
pub use error::ShellError;
pub use headless::{ScreenshotInfo, render_target, save_screenshot, screenshot};
pub use window::run_window;
