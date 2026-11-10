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
                      rendered page (headless screenshots; window: Ctrl+F)
  -h, --help          print this help

window keys:
  left click on a link       navigate (history push)
  left click on a form control: focus; type to edit text (Backspace deletes)
  Tab                        next form control; Enter never submits; Esc blurs
  Ctrl+F                     find in page: type to search, Enter next, Shift+Enter
                             previous, Esc closes; matches are highlighted in yellow
  Enter                      ignored (no backend: forms never submit); Esc blurs
  Alt+Left / Backspace / b   back;  Alt+Right / n   forward
  Ctrl+= / Ctrl++ / = / +    zoom in;  Ctrl+- / -   zoom out;  Ctrl+0 / 0   reset zoom
  Ctrl+L                     print the current URL
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
    let input = raw
        .input
        .ok_or_else(|| ShellError::Usage(format!("missing <url|file>\n{USAGE}")))?;
    let width = raw.width.unwrap_or(DEFAULT_WIDTH);
    let height = raw.height.unwrap_or(DEFAULT_HEIGHT);
    validate_dimensions(width, height)?;
    let input = normalize_input(&input)?;
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
    if net::Url::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    let path = std::fs::canonicalize(input).map_err(|error| {
        ShellError::Usage(format!(
            "cannot resolve `{input}` as a URL or local file: {error}"
        ))
    })?;
    let path = path.display().to_string();
    let url = format!("file://{path}");
    net::Url::parse(&url)
        .map(|parsed| parsed.as_str())
        .map_err(|error| ShellError::Usage(format!("cannot build file URL for {path}: {error}")))
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
}
