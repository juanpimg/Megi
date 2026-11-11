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
//! - **Session & downloads (R150)**: the window persists tabs/history/zoom/scroll to
//!   `$XDG_DATA_HOME/megi/session.json` (or `--session-file PATH`) with debounced atomic
//!   saves, tolerant loading of corrupt files and `--no-session` to disable it; headless
//!   never writes a session. `Ctrl+S` exports the rendered page as
//!   `megi-<host>-<n>.png` and `Ctrl+Shift+S` the post-script DOM as
//!   `megi-<host>-<n>.html` (doctype included) into `--download-dir`/`~/Downloads`;
//!   headless equivalents are `--save-png PATH` / `--save-html PATH` ([`session`],
//!   [`download`]).
//! - **Browser chrome (R135)**: a shell-drawn overlay ([`chrome`]) with a tab strip
//!   (click to activate/close, `+` new tab, `+N` overflow) and an address bar (`Ctrl+L`,
//!   caret editing, `Enter` navigates via `Url::join`, `Esc` cancels), plus the floating
//!   `<select>` dropdown (options anchored under the control, capped height, hover/active
//!   rows, click/Enter updates `selected` in the DOM snapshot and re-renders). The page
//!   viewport is not reduced: the chrome occludes the top `CHROME_HEIGHT` CSS pixels
//!   (documented overlay trade-off). Headless screenshots include it only with `--chrome`.
//! - **Tabs (R120)**: `Ctrl+T` opens a blank tab, `Ctrl+W` closes the active one (the last
//!   tab is reset to New Tab instead of closing the window), `Ctrl+Tab`/`Ctrl+Shift+Tab`
//!   cycle and `Alt+1..9` activates a tab by position. Each [`Tab`] keeps its own history,
//!   title/URL, scroll, zoom and find state ([`tabs`]); switching tabs saves the current
//!   state, restores the target state and requests a fresh generation-cancelled render, so
//!   frames never leak between tabs. The title is prefixed with `[N/M]` when several tabs
//!   are open. `--tabs URL` (repeatable) opens extra tabs; headless `--screenshot` renders
//!   the **active (first)** tab. There is no visual tab bar yet.
//! - **Find in page (WP-7.7)**: `Ctrl+F` searches the rendered DOM text case-insensitively
//!   ([`find`]); typing updates the query, `Enter`/`Shift+Enter` jump to the next/previous
//!   hit with wrap-around, `Esc` closes and the scroll is adjusted so the current hit is
//!   visible. All hits are highlighted yellow `rgba(255, 235, 59, 0.45)` and the current one
//!   more strongly, painted over the frame (window) or into the pixmap before PNG encoding
//!   (`--screenshot --find QUERY`).
//! - **Navigation extras (WP-7.7)**: `F5` and `g` reload, `Alt+Home` navigates back to the
//!   first (home) history entry, `Ctrl+D` prints the final URL and document title; there are
//!   still no tabs.
//! - **Zoom (WP-7.6)**: `Ctrl+=`/`Ctrl++` zoom in, `Ctrl+-` zoom out, `Ctrl+0` reset
//!   (same keys without Ctrl also work) on a fixed 50–300 % ladder ([`zoom`]); the CSS
//!   viewport becomes `physical / (device_scale * zoom)` and the title shows the zoom
//!   percentage. `--zoom <0.5..3.0>` sets the initial factor; `--screenshot` renders at
//!   the zoomed CSS size.
//! - **Auto-refresh (WP-7.6)**: with `--refresh-ms [MS]` (opt-in; `MS` defaults to 250)
//!   the window re-renders the page while the last frame left
//!   `script_timers_pending > 0` (typically `setInterval`), keeping the scroll offset and
//!   using the worker's generation cancellation; pages without timers never loop
//!   ([`refresh`]).
//! - **Status feedback**: the title shows `page — zoom% — scroll%` plus warning/script-error
//!   counters; `Ctrl+L` prints the current URL to stdout (there is no address bar yet).
//! - **Forms (WP-7.7)**: UA-styled inputs/textarea/select/button/checkbox/radio render as
//!   CSS boxes ([`forms`]); clicking focuses a control, typing edits its value, Backspace
//!   deletes, Tab cycles focus, checkboxes/radios toggle their `checked` attribute, buttons
//!   flash a pressed state, and `Enter` never submits (no backend). Edits patch the DOM
//!   snapshot and re-render it as an inline `data:` document with the original page as
//!   resource base (scripts disabled while editing — see [`forms`] limits).
//! - **Input**: mouse wheel/touchpad scroll, `↑`/`↓`, `PageUp`/`PageDown`, `Home`/`End`,
//!   `q`/`Esc`/close quit; right/middle click ignored.
//! - **Errors** produce a white frame, a `Megi — error` title and stderr text; failed
//!   navigations keep their history entry so `back` still works.
//!
//! ## What it does not do yet
//!
//! - No tabs, downloads or address bar; `Ctrl+L` only prints the URL. Forms have no
//!   validation/backend/JS handlers, no caret blink or selection ranges, and `<select>` has
//!   no dropdown (its first option is painted). JS runs in
//!   `render` (page scripts, virtual-time event loop) but the shell does not expose a JS
//!   console or bookmarking; link clicks are the only navigation trigger and
//!   `javascript:`/`mailto:` hrefs are no-ops.
//! - Auto-refresh re-renders with the same deterministic virtual clock: pages whose visual
//!   state depends on real elapsed time (animations, clocks) repaint the same first frame;
//!   advancing the virtual clock between refreshes is a follow-up. Refresh is opt-in via
//!   `--refresh-ms` and only for frames with pending page timers.
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

pub mod chrome;
pub mod cli;
pub mod convert;
pub mod download;
pub mod error;
pub mod find;
pub mod forms;
pub mod headless;
pub mod links;
pub mod navigation;
pub mod present;
pub mod refresh;
pub mod scale;
pub mod scroll;
pub mod session;
pub mod tabs;
pub mod title;
pub mod window;
pub mod worker;
pub mod zoom;

pub use chrome::{
    ADDRESS_BAR_HEIGHT, AddressBar, CHROME_HEIGHT, ChromeView, DropdownList, DropdownState,
    TAB_STRIP_HEIGHT, TabHit, TabRect, TabStripLayout, address_bar_rect, draw_chrome,
    dropdown_layout, in_chrome, layout_tabs, resolve_address, tab_strip_hit, truncate_title,
};
pub use cli::{CliArgs, LaunchMode, finish_args, parse_args, parse_cli};
pub use convert::{PIXEL_FORMAT, expected_buffer_len, pixmap_to_0rgb};
pub use download::{
    DOWNLOAD_PREFIX, DownloadKind, MAX_HOST_CHARS, SavedDownload, default_download_dir,
    download_file_name, is_png, next_download_path, resolve_download_dir, sanitize_host, save_html,
    save_png,
};
pub use error::ShellError;
pub use find::{
    FindMatch, FindState, HIGHLIGHT_ALPHA, HIGHLIGHT_CURRENT_ALPHA, HIGHLIGHT_RGB, highlight_rects,
    overlay_highlights_dest, overlay_highlights_pixmap, scroll_offset_for, search,
};
pub use forms::{
    ACTIVE_ATTR, FOCUS_ATTR, FORM_ID_ATTR, FocusState, FormControl, FormControlKind, TextEdit,
    assign_form_ids, collect_controls, control_at, patch_control, snapshot_data_url,
};
pub use headless::{
    ScreenshotInfo, render_target, render_target_with_chrome, render_target_with_find,
    render_target_with_options, save_screenshot, screenshot, screenshot_with_chrome,
    screenshot_with_find, screenshot_with_options,
};
pub use links::{
    CssRect, LinkError, LinkRegion, collect_link_regions, find_link_at, is_navigable, link_url_at,
    physical_to_page, resolve_href,
};
pub use navigation::{MAX_HISTORY, NavigationOutcome, NavigationState};
pub use present::{present_region, viewport_slice};
pub use refresh::{DEFAULT_REFRESH_MS, MIN_REFRESH_MS, RefreshPolicy, RefreshTimer};
pub use scale::{
    WindowMetrics, logical_to_physical, physical_to_logical, window_metrics, window_metrics_zoomed,
};
pub use scroll::{MAX_CONTENT_HEIGHT, ScrollState};
pub use session::{
    LoadedSession, MAX_HISTORY_PER_TAB, MAX_SESSION_TABS, SCHEMA_VERSION, SESSION_FILE_NAME,
    Session, SessionLock, SessionTab, default_session_path, load as load_session,
    resolve_session_path, save as save_session,
};
pub use tabs::{BLANK_TAB_TITLE, BLANK_TAB_URL, CloseOutcome, MAX_TABS, Tab, TabsState};
pub use title::{
    CAPPED_MARKER, FALLBACK_NAME, MAX_SHORT_URL_CHARS, MAX_TITLE_CHARS, document_title, page_title,
    short_url, window_title,
};
pub use window::{
    WindowViewOptions, run_window, run_window_with_options, run_window_with_view_options,
};
pub use worker::{FrameStore, RenderFrame, RenderRequest, WorkerMessage, spawn_worker};
pub use zoom::{
    DEFAULT_ZOOM, MAX_ZOOM, MIN_ZOOM, ZOOM_LEVELS, is_valid_cli_zoom, sanitize_zoom, zoom_in,
    zoom_label, zoom_out, zoom_reset, zoomed_viewport,
};
