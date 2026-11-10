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
//! ## What the window does today (WP-7.2/7.3/7.5)
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
//! - **Link navigation + history**: a left click on a rendered `<a href>` resolves the
//!   link against the page URL ([`links`]), pushes a [`navigation::NavigationState`] entry
//!   and renders the new URL on the worker (generation-based cancellation). `Alt+←` /
//!   `Backspace` / `b` go back, `Alt+→` / `n` go forward (`r` reloads); history is bounded
//!   by [`MAX_HISTORY`] and navigating from the past truncates forward entries. Link
//!   clickability comes from `render::RenderOutput::hit_regions` (the additive paint
//!   hit-testing layer of the same render pass as the pixels) resolved through
//!   `paint::find_link_at`; a documented layout-tree fallback covers outputs without
//!   regions.
//! - **Browser networking**: `--cookies` enables a session [`net::CookieJar`] shared by
//!   the document, its subresources, redirects and later navigations; `--origin` overrides
//!   the CORS `Origin` used for subresource fetches. `render` derives it from the document
//!   URL by default; CORS denials are non-fatal warnings. `gzip`/`deflate` responses are
//!   decoded transparently by `net`.
//! - **Dynamic title**: the window title is the document `<title>` plus the short URL and
//!   scroll percentage ([`title`]); server redirects replace the current history entry.
//! - **Input**: mouse wheel/touchpad scroll, `↑`/`↓`, `PageUp`/`PageDown`, `Home`/`End`,
//!   `q`/`Esc`/close quit; right/middle click ignored.
//! - **Errors** produce a white frame, a `Megi — error` title and stderr text; failed
//!   navigations keep their history entry so `back` still works.
//!
//! ## What it does not do yet
//!
//! - No tabs, zoom, downloads, forms or JS execution (Phase 6) — link clicks are the only
//!   navigation trigger; `javascript:`/`mailto:` hrefs are no-ops.
//! - Fragment-only links (`#id`) are treated as a normal navigation (a re-render of the same
//!   document with a new history entry) instead of in-page scrolling/scrolling-into-view.
//! - The scroll is a simple vertical offset of the rendered page (no incremental tiled
//!   rendering): pages taller than 8192 CSS px are clipped.
//! - Hit regions are the painted primitives of `render` (text runs/backgrounds/borders/
//!   images inside `<a href>`): `<area>`, CSS `pointer-events`, transforms and hover
//!   states remain out of scope.
//! - Cookies: one session jar per process (`--cookies`), no persistence across processes,
//!   `SameSite` ignored and no Public Suffix List (documented in `net::cookies`). CORS is
//!   the `net` subset (no preflight cache/Expose-Headers); `Origin` only affects
//!   subresource fetches, never the top-level navigation.
//! - Window-mode code is not exercised by tests (no display server in CI); all navigation,
//!   hit-testing and title logic is pure and tested headlessly.
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
pub mod links;
pub mod navigation;
pub mod present;
pub mod scale;
pub mod scroll;
pub mod title;
pub mod window;
pub mod worker;

pub use cli::{CliArgs, LaunchMode, finish_args, parse_args, parse_cli};
pub use convert::{PIXEL_FORMAT, expected_buffer_len, pixmap_to_0rgb};
pub use error::ShellError;
pub use headless::{
    ScreenshotInfo, render_target, render_target_with_options, save_screenshot, screenshot,
    screenshot_with_options,
};
pub use links::{
    CssRect, LinkError, LinkRegion, collect_link_regions, find_link_at, is_navigable, link_url_at,
    physical_to_page, resolve_href,
};
pub use navigation::{MAX_HISTORY, NavigationOutcome, NavigationState};
pub use present::{present_region, viewport_slice};
pub use scale::{WindowMetrics, logical_to_physical, physical_to_logical, window_metrics};
pub use scroll::{MAX_CONTENT_HEIGHT, ScrollState};
pub use title::{
    CAPPED_MARKER, FALLBACK_NAME, MAX_SHORT_URL_CHARS, MAX_TITLE_CHARS, document_title, page_title,
    short_url, window_title,
};
pub use window::{run_window, run_window_with_options};
pub use worker::{FrameStore, RenderFrame, RenderRequest, WorkerMessage, spawn_worker};
