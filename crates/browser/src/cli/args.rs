use super::*;

pub const DEFAULT_WIDTH: u32 = 800;
/// Default window/screenshot height in CSS pixels.
pub const DEFAULT_HEIGHT: u32 = 600;
/// Default window title.
pub const DEFAULT_TITLE: &str = "Megi";

/// Maximum accepted viewport/window dimension (matches [`megi_webview::render::MAX_VIEWPORT_DIMENSION`]).
pub const MAX_DIMENSION: u32 = megi_webview::render::MAX_VIEWPORT_DIMENSION;

/// Help text for the binary.
pub const USAGE: &str = "\
usage: megi <url|file> [options]

  <url|file>          absolute http(s)/file/data URL, or a local HTML file path

options:
  --width W           window/screenshot width in CSS pixels (default 800, max 8192)
  --height H          window/screenshot height in CSS pixels (default 600, max 8192)
  --window            explicitly request interactive window mode (requires `window` feature)
  --remote-debugging-port PORT
                      start the DevTools HTTP/WebSocket endpoint on 127.0.0.1:PORT
                      (JSON-RPC Runtime.evaluate; requires the `full` feature)
  --screenshot PATH   headless mode: render once and write a PNG (no window/display)
  --print-to-pdf PATH headless mode: render once and write PDF 1.4 with the page
                      as an image plus a selectable text layer (no window/display)
  --title T           initial window title (default \"Megi\"; replaced by page title)
  --cookies           enable the session cookie jar (Set-Cookie shared between the
                      document, subresources, redirects and navigations)
  --origin URL        CORS Origin override for subresource fetches (default: the
                      document origin; cross-origin CORS failures are non-fatal)
  --zoom FACTOR       page zoom, 0.5–3.0 (default 1.0); the CSS viewport is
                      physical/(device_scale*zoom) and the title shows the percentage
  --refresh-ms [MS]   auto-refresh pages whose scripts leave timers pending
  --time MS           evaluate CSS animations at MS milliseconds (default 0)
                      (setInterval); MS defaults to 250, 0/disabling flag = off
  --find QUERY        highlight case-insensitive matches of QUERY in the
                      rendered page (headless screenshots; window: Ctrl+F); the
                      PNG is encoded once after the overlay
  --chrome            include the shell chrome (tab strip + address bar) in the
                      headless --screenshot output (window always shows chrome);
                      the page PNG is skipped and encoded once after the overlay
  --chrome-menu       with --chrome, also draw a deterministic open context menu (tests)
  --reader            render the deterministic reader-mode extraction of the page
                      (headless; window: Ctrl+Shift+R)
  --no-session        do not load/save the persistent session
  --session-file PATH session JSON path (default: $XDG_DATA_HOME/megi/session.json)
  --download-dir PATH directory for Ctrl+S PNG / Ctrl+Shift+S HTML downloads
  --save-png PATH     headless: also write the rendered PNG to PATH
  --save-html PATH    headless: also write the serialized DOM (doctype) to PATH
  --tabs URL          open URL as an extra tab (repeatable, max 32); the positional
                      URL is the first tab
  --active-tab N      headless: render tab N (0-based, default 0) instead of the first
  -h, --help          print this help

window keys:
  left click on a link       navigate (history push)
  left click on the tab strip  activate tab; click x to close; click + for a new tab
  left click on a form control: focus; type to edit text (Backspace deletes); a select
                             opens its option dropdown (click/Enter chooses)
  Tab                        next form control; Enter never submits; Esc blurs
  Ctrl+F                     find in page: type to search, Enter next, Shift+Enter
                             previous, Esc closes; matches are highlighted in yellow
  Ctrl+T / Ctrl+W            new tab / close tab (last tab resets to New Tab)
  Ctrl+Tab / Ctrl+Shift+Tab  next / previous tab;  Alt+1..9   activate tab by index
  Enter                      ignored (no backend: forms never submit); Esc blurs
  Alt+Left / Backspace / b   back;  Alt+Right / n   forward
  Ctrl+= / Ctrl++ / = / +    zoom in;  Ctrl+- / -   zoom out;  Ctrl+0 / 0   reset zoom
  Ctrl+L                     edit the address bar: Enter navigates (Url::join), Esc cancels
  r / g / F5                 reload;  Alt+Home   go to the first (home) history entry
  Ctrl+D                     print the final URL and the page title
  q / Esc                    quit
  wheel / arrows / PageUp / PageDown / Home / End   scroll
  (left click only; right/middle click ignored)";

/// What the CLI should do after parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    /// Open the interactive winit window.
    Window,
    /// Headless: render once and write the PNG at this path.
    Screenshot(PathBuf),
    /// Headless: render once and write a one-page PDF at this path.
    PrintPdf(PathBuf),
}

/// Fully validated CLI configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct CliArgs {
    /// Normalized absolute URL (local paths become `file://` URLs).
    pub input: String,
    /// Window title.
    pub title: String,
    /// Requested width in pixels.
    pub width: u32,
    /// Requested height in pixels.
    pub height: u32,
    /// Window, headless screenshot or headless PDF mode.
    pub mode: LaunchMode,
    /// Headless: write the rendered page as a one-page PDF (`--print-to-pdf`).
    pub print_to_pdf: Option<PathBuf>,
    /// Enable the session cookie jar (`--cookies`).
    pub cookies: bool,
    /// Explicit CORS `Origin` override for subresource fetches (`--origin`).
    pub origin: Option<String>,
    /// Initial page zoom factor (50–300 %, `--zoom`).
    pub zoom: f64,
    /// Optional find-in-page query (`--find`; headless screenshots highlight its hits).
    pub find: Option<String>,
    /// Initial tabs (first = positional URL, then each `--tabs` in order).
    pub tabs: Vec<String>,
    /// Zero-based index of the tab rendered in headless screenshot mode (#1689).
    pub active_tab: usize,
    /// Include the shell chrome in headless screenshots (`--chrome`).
    pub chrome: bool,
    /// Draw a deterministic open context menu in headless chrome screenshots.
    pub chrome_menu: bool,
    /// Draw the opt-in compact chrome band in headless screenshots (`--chrome-compact`).
    pub chrome_compact: bool,
    /// Suppress chrome animation (`--reduced-motion`).
    pub reduced_motion: bool,
    /// Render the extracted reader page instead of the raw page (`--reader`).
    pub reader: bool,
    /// Disable session persistence (`--no-session`).
    pub no_session: bool,
    /// Explicit session file (`--session-file`).
    pub session_file: Option<PathBuf>,
    /// Download directory override (`--download-dir`).
    pub download_dir: Option<PathBuf>,
    /// Headless: also write the rendered PNG to this path (`--save-png`).
    pub save_png: Option<PathBuf>,
    /// Headless: also write the serialized DOM to this path (`--save-html`).
    pub save_html: Option<PathBuf>,
    /// Auto-refresh interval in milliseconds when `--refresh-ms` was given (`None` = off).
    pub refresh_ms: Option<u64>,
    /// CSS animation time in milliseconds (`--time`; default 0).
    pub time_ms: f64,
    /// Render headless screenshots in an isolated OS process (`--ipc-render`, ADR-004).
    pub ipc_render: bool,
    /// DevTools endpoint port on loopback (`--remote-debugging-port`), if requested.
    pub remote_debugging_port: Option<u16>,
}

/// Raw flags before validation/normalization (exposed for tests).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawArgs {
    /// Positional input (URL or path), if provided.
    pub input: Option<String>,
    /// `--width` value, if provided.
    pub width: Option<u32>,
    /// `--height` value, if provided.
    pub height: Option<u32>,
    /// `--screenshot` path, if provided.
    pub screenshot: Option<PathBuf>,
    /// `--print-to-pdf` path, if provided.
    pub print_to_pdf: Option<PathBuf>,
    /// `--title` value, if provided.
    pub title: Option<String>,
    /// `--cookies` was passed.
    pub cookies: bool,
    /// `--origin` value, if provided.
    pub origin: Option<String>,
    /// `--zoom` value, if provided.
    pub zoom: Option<f64>,
    /// `--find` value, if provided.
    pub find: Option<String>,
    /// `--tabs` URLs, in order (repeatable).
    pub tabs: Vec<String>,
    /// `--active-tab` value, if provided.
    pub active_tab: Option<usize>,
    /// `--chrome` was passed.
    pub chrome: bool,
    /// `--chrome-menu` was passed.
    pub chrome_menu: bool,
    /// `--chrome-compact` was passed.
    pub chrome_compact: bool,
    /// `--reduced-motion` was passed.
    pub reduced_motion: bool,
    /// `--reader` was passed.
    pub reader: bool,
    /// `--no-session` was passed.
    pub no_session: bool,
    /// `--session-file` value.
    pub session_file: Option<PathBuf>,
    /// `--download-dir` value.
    pub download_dir: Option<PathBuf>,
    /// `--save-png` value.
    pub save_png: Option<PathBuf>,
    /// `--save-html` value.
    pub save_html: Option<PathBuf>,
    /// `--refresh-ms` value (default 250 when passed without a value), if provided.
    pub refresh_ms: Option<u64>,
    /// `--time` value in milliseconds, if provided.
    pub time_ms: Option<f64>,
    /// `--ipc-render` was passed.
    pub ipc_render: bool,
    /// `--remote-debugging-port` value, if provided.
    pub remote_debugging_port: Option<u16>,
    /// `--window` was passed.
    pub window: bool,
    /// `-h`/`--help` was passed.
    pub help: bool,
}
