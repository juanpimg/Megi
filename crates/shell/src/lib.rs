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
//! ## What the window does today (WP-7.2/7.3)
//!
//! - **Worker thread**: the page is rendered off the event loop
//!   ([`worker`]); requests carry a generation, obsolete results are discarded,
//!   and rapid requests are coalesced. The event loop is woken through a winit
//!   `EventLoopProxy` user event (no polling).
//! - **Full-page render + vertical scroll**: the document is rendered at the CSS
//!   viewport width and its full content height (capped at
//!   [`scroll::MAX_CONTENT_HEIGHT`], the render pipeline's 8192 px viewport
//!   limit; taller pages are clipped with a `[recortado]` title marker). The
//!   visible region is presented with [`present::present_region`].
//! - **HiDPI**: the CSS viewport is `physical / scale_factor` ([`scale`]) and
//!   the CSS frame is upscaled to physical pixels with nearest-neighbour
//!   sampling. Runtime scale changes (moving between monitors) trigger a
//!   debounced re-render.
//! - **Input**: mouse wheel/touchpad scroll, `↑`/`↓`, `PageUp`/`PageDown`,
//!   `Home`/`End`, `r` re-render, `q`/`Esc`/close quit, left click prints the
//!   current URL to stdout.
//! - **Errors** produce a white frame, a `Megi — error` title and stderr text;
//!   the window stays usable.
//!
//! ## What it does not do yet
//!
//! No DOM hit-testing, link navigation, history, tabs, zoom, JS or forms. The
//! scroll is a simple vertical offset of the rendered page (no incremental
//! tiled rendering): pages taller than 8192 CSS px are clipped. Window-mode
//! code is not exercised by tests (no display server in CI).
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
pub mod present;
pub mod scale;
pub mod scroll;
pub mod window;
pub mod worker;

pub use cli::{CliArgs, LaunchMode, finish_args, parse_args, parse_cli};
pub use convert::{PIXEL_FORMAT, expected_buffer_len, pixmap_to_0rgb};
pub use error::ShellError;
pub use headless::{ScreenshotInfo, render_target, save_screenshot, screenshot};
pub use present::{present_region, viewport_slice};
pub use scale::{WindowMetrics, logical_to_physical, physical_to_logical, window_metrics};
pub use scroll::{MAX_CONTENT_HEIGHT, ScrollState};
pub use window::run_window;
pub use worker::{FrameStore, RenderFrame, RenderRequest, WorkerMessage, spawn_worker};
