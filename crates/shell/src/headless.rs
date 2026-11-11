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

/// Renders `input` and paints the shell chrome (tab strip + address bar) over the page.
///
/// Headless equivalent of the window overlay: the chrome occupies the top
/// [`crate::chrome::CHROME_HEIGHT`] CSS pixels and the page keeps its full viewport
/// (documented overlay trade-off). Used by `--screenshot --chrome` for tests; the PNG
/// returned by [`render_target`] stays chrome-free so page goldens are unaffected.
pub fn render_target_with_chrome(
    input: &str,
    width: u32,
    height: u32,
    options: &render::RenderOptions,
    find_query: Option<&str>,
) -> Result<render::RenderOutput, ShellError> {
    let mut output = render_target_with_options(input, width, height, options)?;
    if let Some(query) = find_query {
        let rects = crate::find::highlight_rects(&output.layout, query);
        crate::find::overlay_highlights_pixmap(&mut output.pixmap, &rects, 0);
    }
    let mut dest = crate::convert::pixmap_to_0rgb(&output.pixmap);
    let title = crate::title::document_title(&output.document).unwrap_or_else(|| input.to_string());
    let tabs = vec![crate::chrome::truncate_title(&title, 24)];
    let address = crate::chrome::AddressBar::new();
    let view = crate::chrome::ChromeView {
        tabs: &tabs,
        active_tab: 0,
        address: &address,
        fallback_url: input,
        dropdown: None,
        scroll_y: 0,
        css_viewport_height: height,
        css_width: width as f32,
        completion: None,
        loading: false,
        overflow: None,
    };
    crate::chrome::draw_chrome(&mut dest, width, height, 1.0, &view);
    output.pixmap.pixels = dest
        .iter()
        .map(|pixel| {
            paint::Color::new(
                ((pixel >> 16) & 0xFF) as u8,
                ((pixel >> 8) & 0xFF) as u8,
                (pixel & 0xFF) as u8,
                0xFF,
            )
        })
        .collect();
    Ok(output)
}

/// Headless screenshot including the shell chrome (`--screenshot --chrome`).
pub fn screenshot_with_chrome(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    options: &render::RenderOptions,
    find_query: Option<&str>,
) -> Result<ScreenshotInfo, ShellError> {
    let output = render_target_with_chrome(input, width, height, options, find_query)?;
    let png = output.pixmap.to_png();
    let path = path.as_ref().to_path_buf();
    if png.is_empty() {
        return Err(ShellError::Io {
            path: path.display().to_string(),
            reason: "PNG encoding produced no bytes".to_string(),
        });
    }
    write_bytes(&path, &png)?;
    Ok(ScreenshotInfo {
        path,
        width: output.pixmap.width,
        height: output.pixmap.height,
        png_bytes: png.len(),
    })
}

/// Renders `input` and paints find-in-page highlights for `query` into the pixmap.
///
/// Returns the render output (with the highlight overlay already blended into
/// [`render::RenderOutput::pixmap`]) and the number of hits. The PNG field of the output is
/// **not** updated: use [`screenshot_with_find`] or [`render::RenderOutput::pixmap`]'s
/// [`paint::to_png`] for an encoded image with highlights.
pub fn render_target_with_find(
    input: &str,
    width: u32,
    height: u32,
    options: &render::RenderOptions,
    query: &str,
) -> Result<(render::RenderOutput, usize), ShellError> {
    let mut output = render_target_with_options(input, width, height, options)?;
    let hits = crate::find::search(&output.document, query).len();
    let rects = crate::find::highlight_rects(&output.layout, query);
    crate::find::overlay_highlights_pixmap(&mut output.pixmap, &rects, 0);
    Ok((output, hits))
}

/// Headless screenshot with find-in-page highlights (`--find QUERY`).
///
/// The highlights are painted into the CSS-resolution pixmap before encoding, so the PNG
/// shows yellow rectangles for every hit and a stronger one for the first hit.
pub fn screenshot_with_find(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    options: &render::RenderOptions,
    query: &str,
) -> Result<ScreenshotInfo, ShellError> {
    let (output, hits) = render_target_with_find(input, width, height, options, query)?;
    let png = output.pixmap.to_png();
    let path = path.as_ref().to_path_buf();
    if png.is_empty() {
        return Err(ShellError::Io {
            path: path.display().to_string(),
            reason: "PNG encoding produced no bytes".to_string(),
        });
    }
    write_bytes(&path, &png)?;
    let _ = hits;
    Ok(ScreenshotInfo {
        path,
        width: output.pixmap.width,
        height: output.pixmap.height,
        png_bytes: png.len(),
    })
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
