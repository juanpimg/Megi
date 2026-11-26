//! Headless chrome-band compositing, shared by the default and compact bands.
//!
//! Both modes render the page in the viewport left below the band and composite
//! it into a full-size canvas, so the top of the page is never painted under
//! the chrome. Kept in a submodule so the parent stays small.

use super::*;

/// Explicit opt-in compact chrome band (`--screenshot --chrome --chrome-compact`).
/// Both this mode and [`super::render_target_with_chrome`] reserve the band and
/// composite the page below it; only the band height differs.
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
    // Real browsers shrink the viewport by the chrome band and the sidebar column:
    // the page is laid out in that reduced viewport and composited into the frame.
    let band_css = crate::chrome::band_css_height(compact);
    let band_rows = (band_css.ceil() as u32).min(height);
    let sidebar_cols =
        (crate::chrome::sidebar_width(width as f32).ceil() as u32).min(width.saturating_sub(1));
    let page_width = width.saturating_sub(sidebar_cols).max(1);
    let bottom_margin = (crate::chrome::WINDOW_MARGIN.ceil() as u32).min(height);
    let page_height = height
        .saturating_sub(band_rows.saturating_add(bottom_margin))
        .max(1);

    // Chrome/find paint the pixmap after the pipeline, so skip the eager page PNG and
    // encode once at the end (R201).
    let mut base = options.clone();
    base.encode_png = false;
    let mut output = render_target_with_options(input, page_width, page_height, &base)?;
    if let Some(query) = find_query {
        let rects = crate::find::highlight_rects(&output.layout, query);
        // Highlights are painted in page coordinates before the blit; copying the
        // page below the band shifts them down by `band_rows` along with the page.
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
        css_viewport_height: page_height,
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
        shell: Some(crate::chrome::ShellView {
            labels: &tabs,
            favicons: &favicons,
            url: input,
            active: 0,
            top_css: band_css,
            sidebar: true,
        }),
    };
    // Blit the reduced page into a full-size canvas beside the sidebar and below the
    // band, then paint the shell layer. The simple `--chrome` view has no dropdown,
    // status or menu overlays, so the RGBA compositor skips the full 0RGB round-trip.
    let page = &output.pixmap;
    let mut canvas = megi_webview::paint::Pixmap::new_with_background(
        width,
        height,
        megi_webview::paint::Color::WHITE,
    );
    crate::chrome::paint_window_frame(&mut canvas, 1.0, width as f32, height as f32);
    let columns = page.width.min(canvas.width.saturating_sub(sidebar_cols)) as usize;
    let rows = page
        .height
        .min(canvas.height.saturating_sub(band_rows + bottom_margin)) as usize;
    for row in 0..rows {
        let src = row * page.width as usize;
        let dst = (band_rows as usize + row) * canvas.width as usize + sidebar_cols as usize;
        canvas.pixels[dst..dst + columns].copy_from_slice(&page.pixels[src..src + columns]);
    }
    output.pixmap = canvas;
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
