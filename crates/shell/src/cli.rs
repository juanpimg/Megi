//! CLI argument parsing and URL/path normalization (pure, testable, no display needed).
//!
//! ```text
//! megi <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
//! ```
//!
//! Without `--screenshot` the CLI opens a winit window; with it, Megi runs headless and
//! writes a PNG (no `EventLoop` is created). Local filesystem paths are converted to
//! `file://` URLs; anything that parses as an absolute URL is used as-is.

use std::path::PathBuf;

use crate::error::ShellError;

/// Default window/screenshot width in CSS pixels.
pub const DEFAULT_WIDTH: u32 = 800;
/// Default window/screenshot height in CSS pixels.
pub const DEFAULT_HEIGHT: u32 = 600;
/// Default window title.
pub const DEFAULT_TITLE: &str = "Megi";

/// Maximum accepted viewport/window dimension (matches [`render::MAX_VIEWPORT_DIMENSION`]).
pub const MAX_DIMENSION: u32 = render::MAX_VIEWPORT_DIMENSION;

/// Help text for the binary.
pub const USAGE: &str = "\
usage: megi <url|file> [options]

  <url|file>          absolute http(s)/file/data URL, or a local HTML file path

options:
  --width W           window/screenshot width in CSS pixels (default 800, max 8192)
  --height H          window/screenshot height in CSS pixels (default 600, max 8192)
  --screenshot PATH   headless mode: render once and write a PNG (no window/display)
  --title T           initial window title (default \"Megi\"; replaced by page title)
  --cookies           enable the session cookie jar (Set-Cookie shared between the
                      document, subresources, redirects and navigations)
  --origin URL        CORS Origin override for subresource fetches (default: the
                      document origin; cross-origin CORS failures are non-fatal)
  --zoom FACTOR       page zoom, 0.5–3.0 (default 1.0); the CSS viewport is
                      physical/(device_scale*zoom) and the title shows the percentage
  --refresh-ms [MS]   auto-refresh pages whose scripts leave timers pending
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
                      URL is the first/active tab, headless renders the active tab
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
    /// Window or headless screenshot mode.
    pub mode: LaunchMode,
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
    /// Include the shell chrome in headless screenshots (`--chrome`).
    pub chrome: bool,
    /// Draw a deterministic open context menu in headless chrome screenshots.
    pub chrome_menu: bool,
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
    /// `--chrome` was passed.
    pub chrome: bool,
    /// `--chrome-menu` was passed.
    pub chrome_menu: bool,
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
    /// `-h`/`--help` was passed.
    pub help: bool,
}

/// Parses raw arguments (no normalization, no defaults applied).
pub fn parse_args(args: impl Iterator<Item = String>) -> Result<RawArgs, ShellError> {
    let arguments: Vec<String> = args.collect();
    let mut raw = RawArgs::default();
    let mut index = 0usize;
    while index < arguments.len() {
        let argument = arguments[index].clone();
        index += 1;
        match argument.as_str() {
            "-h" | "--help" => raw.help = true,
            "--width" => raw.width = Some(parse_value_u32(&arguments, &mut index, "--width")?),
            "--height" => raw.height = Some(parse_value_u32(&arguments, &mut index, "--height")?),
            "--screenshot" => {
                let value = next_value(&arguments, &mut index, "--screenshot")?;
                raw.screenshot = Some(PathBuf::from(value));
            }
            "--title" => {
                raw.title = Some(next_value(&arguments, &mut index, "--title")?);
            }
            "--cookies" => raw.cookies = true,
            "--origin" => {
                raw.origin = Some(next_value(&arguments, &mut index, "--origin")?);
            }
            "--find" => {
                raw.find = Some(next_value(&arguments, &mut index, "--find")?);
            }
            "--tabs" => {
                raw.tabs.push(next_value(&arguments, &mut index, "--tabs")?);
            }
            "--chrome" => raw.chrome = true,
            "--chrome-menu" => raw.chrome_menu = true,
            "--reader" => raw.reader = true,
            "--no-session" => raw.no_session = true,
            "--session-file" => {
                raw.session_file = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--session-file",
                )?));
            }
            "--download-dir" => {
                raw.download_dir = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--download-dir",
                )?));
            }
            "--save-png" => {
                raw.save_png = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--save-png",
                )?));
            }
            "--save-html" => {
                raw.save_html = Some(PathBuf::from(next_value(
                    &arguments,
                    &mut index,
                    "--save-html",
                )?));
            }
            "--zoom" => {
                let value = next_value(&arguments, &mut index, "--zoom")?;
                raw.zoom = Some(value.parse::<f64>().map_err(|_| {
                    ShellError::Usage(format!(
                        "invalid --zoom value `{value}` (expected a number)"
                    ))
                })?);
            }
            "--refresh-ms" => {
                // Optional value: use the next argument when it is a bare integer, otherwise
                // fall back to the default interval without consuming it.
                let interval = arguments
                    .get(index)
                    .and_then(|value| value.parse::<u64>().ok());
                if interval.is_some() {
                    index += 1;
                }
                raw.refresh_ms = Some(interval.unwrap_or(crate::refresh::DEFAULT_REFRESH_MS));
            }
            other if other.starts_with('-') => {
                return Err(ShellError::Usage(format!("unknown option `{other}`")));
            }
            other => {
                if raw.input.is_some() {
                    return Err(ShellError::Usage(format!(
                        "unexpected extra positional argument `{other}`"
                    )));
                }
                raw.input = Some(other.to_string());
            }
        }
    }
    Ok(raw)
}

/// Consumes the value of a CLI flag.
fn next_value(arguments: &[String], index: &mut usize, flag: &str) -> Result<String, ShellError> {
    let value = arguments
        .get(*index)
        .cloned()
        .ok_or_else(|| ShellError::Usage(format!("{flag} requires a value")))?;
    *index += 1;
    Ok(value)
}

/// Consumes and parses a `u32` CLI flag value.
fn parse_value_u32(arguments: &[String], index: &mut usize, flag: &str) -> Result<u32, ShellError> {
    let value = next_value(arguments, index, flag)?;
    value
        .parse::<u32>()
        .map_err(|_| ShellError::Usage(format!("invalid {flag} value `{value}`")))
}

/// Parses and validates the full CLI configuration (`--help` is an error here).
pub fn parse_cli(args: impl Iterator<Item = String>) -> Result<CliArgs, ShellError> {
    finish_args(parse_args(args)?)
}

/// Applies defaults/validation/normalization to [`RawArgs`].
///
/// The binary calls [`parse_args`] first so it can handle `--help` before this step.
pub fn finish_args(raw: RawArgs) -> Result<CliArgs, ShellError> {
    if raw.help {
        return Err(ShellError::Usage(USAGE.to_string()));
    }
    let mut extra_tabs = Vec::with_capacity(raw.tabs.len());
    for tab in &raw.tabs {
        extra_tabs.push(normalize_input(tab)?);
    }
    let (input, tabs) = match raw.input {
        Some(input) => {
            let input = normalize_input(&input)?;
            let mut tabs = vec![input.clone()];
            tabs.extend(extra_tabs);
            (input, tabs)
        }
        None if !extra_tabs.is_empty() => {
            let input = extra_tabs[0].clone();
            (input, extra_tabs)
        }
        None => {
            return Err(ShellError::Usage(format!("missing <url|file>\n{USAGE}")));
        }
    };
    let width = raw.width.unwrap_or(DEFAULT_WIDTH);
    let height = raw.height.unwrap_or(DEFAULT_HEIGHT);
    validate_dimensions(width, height)?;
    let mode = match raw.screenshot {
        Some(path) => LaunchMode::Screenshot(path),
        None => LaunchMode::Window,
    };
    let zoom = match raw.zoom {
        Some(zoom) => {
            if !crate::zoom::is_valid_cli_zoom(zoom) {
                return Err(ShellError::Usage(format!(
                    "invalid --zoom value `{zoom}` (expected {min}..={max})",
                    min = crate::zoom::MIN_ZOOM,
                    max = crate::zoom::MAX_ZOOM
                )));
            }
            zoom
        }
        None => crate::zoom::DEFAULT_ZOOM,
    };
    if let Some(0) = raw.refresh_ms {
        // `--refresh-ms 0` explicitly disables auto-refresh; normalise to `None`.
        return Ok(CliArgs {
            input,
            title: raw.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
            width,
            height,
            mode,
            cookies: raw.cookies,
            origin: raw.origin,
            zoom,
            refresh_ms: None,
            find: raw.find,
            tabs,
            chrome: raw.chrome,
            chrome_menu: raw.chrome_menu,
            reader: raw.reader,
            no_session: raw.no_session,
            session_file: raw.session_file,
            download_dir: raw.download_dir,
            save_png: raw.save_png,
            save_html: raw.save_html,
        });
    }
    Ok(CliArgs {
        input,
        title: raw.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
        width,
        height,
        mode,
        cookies: raw.cookies,
        origin: raw.origin,
        zoom,
        refresh_ms: raw.refresh_ms,
        find: raw.find,
        tabs,
        chrome: raw.chrome,
        chrome_menu: raw.chrome_menu,
        reader: raw.reader,
        no_session: raw.no_session,
        session_file: raw.session_file,
        download_dir: raw.download_dir,
        save_png: raw.save_png,
        save_html: raw.save_html,
    })
}

/// Validates the requested viewport/window dimensions.
pub fn validate_dimensions(width: u32, height: u32) -> Result<(), ShellError> {
    if !(1..=MAX_DIMENSION).contains(&width) || !(1..=MAX_DIMENSION).contains(&height) {
        return Err(ShellError::Usage(format!(
            "invalid dimensions {width}x{height} (each axis must be 1..={MAX_DIMENSION})"
        )));
    }
    Ok(())
}

/// Normalizes the CLI input into an absolute URL.
///
/// A string that parses as an absolute URL is used as-is; otherwise it is treated as a
/// filesystem path, canonicalized and converted to a `file://` URL. The render URL parser
/// percent-encodes spaces and non-ASCII path bytes.
pub fn normalize_input(input: &str) -> Result<String, ShellError> {
    if crate::history::is_history_url(input) {
        return Ok(input.to_string());
    }
    if net::Url::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    let path = std::fs::canonicalize(input).map_err(|error| {
        ShellError::Usage(format!(
            "cannot resolve `{input}` as a URL or local file: {error}"
        ))
    })?;
    // Escape `#`, `?`, `%` and spaces: a raw `file:///tmp/a#b.html` parses as path
    // `/tmp/a` with fragment `b.html` and loads the wrong file.
    let url = crate::pages::file_url(&path);
    net::Url::parse(&url)
        .map(|parsed| parsed.as_str())
        .map_err(|error| {
            ShellError::Usage(format!(
                "cannot build file URL for {}: {error}",
                path.display()
            ))
        })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values.iter().map(|value| (*value).to_string())
    }

    #[test]
    fn parses_defaults() {
        let cli = parse_cli(args(&["data:text/html,<p>hi</p>"])).expect("parse");
        assert_eq!(cli.width, DEFAULT_WIDTH);
        assert_eq!(cli.height, DEFAULT_HEIGHT);
        assert_eq!(cli.title, DEFAULT_TITLE);
        assert_eq!(cli.mode, LaunchMode::Window);
        assert!(cli.input.starts_with("data:"));
    }

    #[test]
    fn parses_all_flags() {
        let cli = parse_cli(args(&[
            "https://example.test/",
            "--width",
            "640",
            "--height",
            "400",
            "--title",
            "Test Window",
            "--screenshot",
            "/tmp/out.png",
        ]))
        .expect("parse");
        assert_eq!(cli.width, 640);
        assert_eq!(cli.height, 400);
        assert_eq!(cli.title, "Test Window");
        assert_eq!(
            cli.mode,
            LaunchMode::Screenshot(PathBuf::from("/tmp/out.png"))
        );
        assert_eq!(cli.input, "https://example.test/");
    }

    #[test]
    fn parses_cookies_and_origin_flags() {
        let cli = parse_cli(args(&[
            "https://example.test/",
            "--cookies",
            "--origin",
            "https://app.example",
        ]))
        .expect("parse");
        assert!(cli.cookies);
        assert_eq!(cli.origin.as_deref(), Some("https://app.example"));
        let plain = parse_cli(args(&["data:,x"])).expect("parse");
        assert!(!plain.cookies);
        assert!(plain.origin.is_none());
        assert!(parse_cli(args(&["data:,x", "--origin"])).is_err());
    }

    #[test]
    fn parses_session_and_download_flags() {
        let cli = parse_cli(args(&[
            "data:,x",
            "--no-session",
            "--session-file",
            "/tmp/s.json",
            "--download-dir",
            "/tmp/dl",
            "--save-png",
            "/tmp/a.png",
            "--save-html",
            "/tmp/a.html",
        ]))
        .expect("parse");
        assert!(cli.no_session);
        assert_eq!(
            cli.session_file.as_deref(),
            Some(std::path::Path::new("/tmp/s.json"))
        );
        assert_eq!(
            cli.download_dir.as_deref(),
            Some(std::path::Path::new("/tmp/dl"))
        );
        assert_eq!(
            cli.save_png.as_deref(),
            Some(std::path::Path::new("/tmp/a.png"))
        );
        assert_eq!(
            cli.save_html.as_deref(),
            Some(std::path::Path::new("/tmp/a.html"))
        );
        let defaults = parse_cli(args(&["data:,x"])).expect("parse");
        assert!(!defaults.no_session);
        assert!(defaults.session_file.is_none());
        assert!(USAGE.contains("--no-session"));
        assert!(USAGE.contains("--save-html"));
    }

    #[test]
    fn parses_chrome_flag() {
        let cli = parse_cli(args(&["data:,x", "--chrome"])).expect("parse");
        assert!(cli.chrome);
        assert!(!parse_cli(args(&["data:,x"])).expect("parse").chrome);
        assert!(USAGE.contains("--chrome"));
    }

    #[test]
    fn parses_repeatable_tabs_flag_in_order() {
        let cli = parse_cli(args(&[
            "data:text/html,first",
            "--tabs",
            "data:text/html,second",
            "--tabs",
            "data:text/html,third",
        ]))
        .expect("parse");
        assert_eq!(cli.tabs.len(), 3);
        assert_eq!(cli.input, "data:text/html,first");
        assert_eq!(cli.tabs[1], "data:text/html,second");
        assert_eq!(cli.tabs[2], "data:text/html,third");

        // Without a positional URL the first `--tabs` becomes the active input.
        let only_tabs = parse_cli(args(&["--tabs", "data:text/html,only"])).expect("parse");
        assert_eq!(only_tabs.input, "data:text/html,only");
        assert_eq!(only_tabs.tabs, vec!["data:text/html,only".to_string()]);

        assert!(parse_cli(args(&["--tabs"])).is_err());
        assert!(USAGE.contains("--tabs"));
        assert!(USAGE.contains("Ctrl+T"));
        assert!(USAGE.contains("Alt+1..9"));
    }

    #[test]
    fn parses_find_flag_and_documents_controls() {
        let cli = parse_cli(args(&["data:,x", "--find", "quick"])).expect("parse");
        assert_eq!(cli.find.as_deref(), Some("quick"));
        let plain = parse_cli(args(&["data:,x"])).expect("parse");
        assert!(plain.find.is_none());
        assert!(parse_cli(args(&["data:,x", "--find"])).is_err());
        assert!(USAGE.contains("--find"));
        assert!(USAGE.contains("Ctrl+F"));
    }

    #[test]
    fn parses_zoom_and_refresh_flags() {
        let cli = parse_cli(args(&["data:,x", "--zoom", "1.5"])).expect("parse");
        assert_eq!(cli.zoom, 1.5);
        assert_eq!(cli.refresh_ms, None);

        let bare = parse_cli(args(&["data:,x", "--refresh-ms"])).expect("parse");
        assert_eq!(bare.refresh_ms, Some(crate::refresh::DEFAULT_REFRESH_MS));

        let explicit = parse_cli(args(&["data:,x", "--refresh-ms", "500"])).expect("parse");
        assert_eq!(explicit.refresh_ms, Some(500));

        let disabled = parse_cli(args(&["data:,x", "--refresh-ms", "0"])).expect("parse");
        assert_eq!(disabled.refresh_ms, None);

        assert!(parse_cli(args(&["data:,x", "--zoom", "4"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--zoom", "0.4"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--zoom", "abc"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--zoom"])).is_err());
    }

    #[test]
    fn rejects_bad_dimensions() {
        assert!(parse_cli(args(&["data:,x", "--width", "0"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--height", "9000"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--width", "abc"])).is_err());
        assert!(parse_cli(args(&["data:,x", "--width"])).is_err());
    }

    #[test]
    fn rejects_unknown_flags_and_missing_input() {
        assert!(parse_cli(args(&["data:,x", "--bogus"])).is_err());
        assert!(parse_cli(args(&[])).is_err());
        assert!(parse_cli(args(&["data:,x", "extra"])).is_err());
    }

    #[test]
    fn help_is_reported() {
        assert!(parse_cli(args(&["--help"])).is_err());
        let raw = parse_args(args(&["-h"])).expect("raw parse");
        assert!(raw.help);
    }

    #[test]
    fn local_paths_become_file_urls() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/render/basic.html");
        let url = normalize_input(&path.display().to_string()).expect("normalize");
        assert!(url.starts_with("file:///"), "unexpected URL: {url}");
        assert!(url.ends_with("tests/fixtures/render/basic.html"));
    }

    #[test]
    fn missing_path_is_a_usage_error() {
        assert!(normalize_input("/definitely/not/a/file.html").is_err());
    }
    #[test]
    fn local_paths_with_url_delimiters_are_escaped() {
        let dir = std::env::temp_dir().join(format!("megi-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("a#b?.html");
        std::fs::write(&path, "<p>x</p>").expect("write");
        let url = normalize_input(&path.display().to_string()).expect("normalize");
        assert!(url.contains("%23"), "fragment delimiter escaped: {url}");
        assert!(url.contains("%3F"), "query delimiter escaped: {url}");
        let parsed = net::Url::parse(&url).expect("url");
        assert!(
            parsed.path().contains("%23") && parsed.path().contains("%3F"),
            "delimiters stay inside the path: {}",
            parsed.path()
        );
        assert!(parsed.query().is_none(), "no query component");
        assert!(parsed.fragment().is_none(), "no fragment component");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
