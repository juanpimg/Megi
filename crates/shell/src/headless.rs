//! Headless rendering and screenshot writing (no display server, no `EventLoop`).
//!
//! This is the path used by `megi <url|file> --screenshot out.png` and by CI: it calls
//! [`render::render_url`] with the requested viewport and writes the deterministic PNG.
//! It is pure library code so the CLI can be tested in fully headless environments.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::ShellError;

/// Metadata of a written screenshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenshotInfo {
    /// Output file path.
    pub path: PathBuf,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Size of the written PNG in bytes.
    pub png_bytes: usize,
}

/// Renders `input` (URL or `file://`) at `width x height` and returns the render output.
///
/// Validates the viewport through [`render::Viewport`] (typed error instead of a huge
/// allocation) and uses [`render::RenderOptions::default`] limits.
pub fn render_target(
    input: &str,
    width: u32,
    height: u32,
) -> Result<render::RenderOutput, ShellError> {
    render_target_with_options(input, width, height, &render::RenderOptions::default())
}

/// [`render_target`] with explicit render options (cookies, CORS origin, scripting, ...).
///
/// The viewport is validated from `width`/`height`; every other option is taken from
/// `options` (e.g. a shared [`net::CookieJar`] or a CORS `origin`).
pub fn render_target_with_options(
    input: &str,
    width: u32,
    height: u32,
    options: &render::RenderOptions,
) -> Result<render::RenderOutput, ShellError> {
    let viewport = render::Viewport::new(width, height)?;
    let mut effective = options.clone();
    effective.viewport = viewport;
    Ok(render::render_url(input, &effective)?)
}

/// Writes the PNG of a finished render to `path` and returns its metadata.
pub fn save_screenshot(
    output: &render::RenderOutput,
    path: impl AsRef<Path>,
) -> Result<ScreenshotInfo, ShellError> {
    let path = path.as_ref().to_path_buf();
    std::fs::write(&path, &output.png).map_err(|error| ShellError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    })?;
    Ok(ScreenshotInfo {
        path,
        width: output.pixmap.width,
        height: output.pixmap.height,
        png_bytes: output.png.len(),
    })
}

/// Convenience end-to-end headless screenshot: render then write the PNG.
pub fn screenshot(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
) -> Result<ScreenshotInfo, ShellError> {
    screenshot_with_options(
        input,
        width,
        height,
        path,
        &render::RenderOptions::default(),
    )
}

/// [`screenshot`] with explicit render options (see [`render_target_with_options`]).
pub fn screenshot_with_options(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    options: &render::RenderOptions,
) -> Result<ScreenshotInfo, ShellError> {
    let output = render_target_with_options(input, width, height, options)?;
    save_screenshot(&output, path)
}

/// Renders an error page filled with white pixels (used by the window fallback).
///
/// Kept here (instead of `window`) so it is testable without a display server.
pub fn error_frame(width: u32, height: u32) -> Vec<u32> {
    let len = crate::convert::expected_buffer_len(width, height).unwrap_or(0);
    crate::convert::blank_frame(len)
}

/// Writes raw bytes to a path, mapping I/O errors to [`ShellError::Io`].
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), ShellError> {
    std::fs::write(path, bytes).map_err(|error: io::Error| ShellError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    })
}
