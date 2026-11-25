//! Explicit opt-in compact chrome screenshot path (Phase 2).
//!
//! Kept in a submodule so the default headless chrome path in the parent stays
//! small and byte-identical to the historical implementation.

use super::*;

/// Explicit opt-in compact chrome band (`--screenshot --chrome --chrome-compact`).
/// The default [`super::render_target_with_chrome`] path and its golden are untouched.
pub fn render_target_with_chrome_compact(
    input: &str,
    width: u32,
    height: u32,
    options: &megi_webview::render::RenderOptions,
    find_query: Option<&str>,
) -> Result<megi_webview::render::RenderOutput, ShellError> {
    render_target_with_chrome_mode(input, width, height, options, find_query, true)
}

pub(super) fn render_target_with_chrome_mode(
    input: &str,
    width: u32,
    height: u32,
    options: &megi_webview::render::RenderOptions,
    find_query: Option<&str>,
    compact: bool,
) -> Result<megi_webview::render::RenderOutput, ShellError> {
    // Chrome/find paint the pixmap after the pipeline, so skip the eager page PNG and
    // encode once at the end (R201); the final bytes are identical to the previous path.
    let mut base = options.clone();
    base.encode_png = false;
    let mut output = render_target_with_options(input, width, height, &base)?;
    if let Some(query) = find_query {
        let rects = crate::find::highlight_rects(&output.layout, query);
        crate::find::overlay_highlights_pixmap(&mut output.pixmap, &rects, 0);
    }
    let title = crate::title::document_title(&output.document).unwrap_or_else(|| input.to_string());
    let tabs = vec![crate::chrome::truncate_title(&title, 24)];
    let favicons = vec![Some(crate::favicon::for_page(
        &output.document,
        &output.document_url,
        options,
    ))];
    let address = crate::chrome::AddressBar::new();
    let view = crate::chrome::ChromeView {
        favicons: &favicons,
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
        hover_url: None,
        menu: None,
        can_go_back: false,
        can_go_forward: false,
        history_menu: None,
        download_status: None,
        history_button: false,
        ime_preedit: "",
        compact,
        rail: None,
    };
    // Only the chrome band is painted: the simple `--chrome` view has no
    // dropdown/status/menu overlays, so the direct RGBA compositor skips the
    // full-framebuffer `0RGB` round-trip and the whole-frame draw (R196).
    let band_css = crate::chrome::band_css_height(compact);
    let band_rows = (band_css.ceil() as u32).min(height);
    crate::chrome::draw_chrome_over_pixmap(&mut output.pixmap, band_rows, 1.0, &view);
    output.ensure_png();
    Ok(output)
}

/// Headless screenshot with the explicit compact chrome band (`--chrome-compact`).
pub fn screenshot_with_chrome_compact(
    input: &str,
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    options: &megi_webview::render::RenderOptions,
    find_query: Option<&str>,
) -> Result<ScreenshotInfo, ShellError> {
    let mut output = render_target_with_chrome_compact(input, width, height, options, find_query)?;
    let png = output.png_bytes().to_vec();
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
