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
  --title T           window title (default \"Megi\")
  -h, --help          print this help

window keys: q / Esc quit, r re-render, arrows/PageUp/PageDown/Home/End scroll
             (mouse wheel/touchpad scrolls; left click prints the current URL)";

/// What the CLI should do after parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    /// Open the interactive winit window.
    Window,
    /// Headless: render once and write the PNG at this path.
    Screenshot(PathBuf),
}

/// Fully validated CLI configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

/// Raw flags before validation/normalization (exposed for tests).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// `-h`/`--help` was passed.
    pub help: bool,
}

/// Parses raw arguments (no normalization, no defaults applied).
pub fn parse_args(args: impl Iterator<Item = String>) -> Result<RawArgs, ShellError> {
    let mut raw = RawArgs::default();
    let mut args = args;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => raw.help = true,
            "--width" => raw.width = Some(parse_dimension(&mut args, "--width")?),
            "--height" => raw.height = Some(parse_dimension(&mut args, "--height")?),
            "--screenshot" => {
                let value = args.next().ok_or_else(|| {
                    ShellError::Usage("--screenshot requires an output path".to_string())
                })?;
                raw.screenshot = Some(PathBuf::from(value));
            }
            "--title" => {
                let value = args
                    .next()
                    .ok_or_else(|| ShellError::Usage("--title requires a value".to_string()))?;
                raw.title = Some(value);
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

fn parse_dimension(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, ShellError> {
    let value = args
        .next()
        .ok_or_else(|| ShellError::Usage(format!("{flag} requires a pixel value")))?;
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
    Ok(CliArgs {
        input,
        title: raw.title.unwrap_or_else(|| DEFAULT_TITLE.to_string()),
        width,
        height,
        mode,
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
