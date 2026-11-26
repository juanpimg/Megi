//! Framebuffer presentation: choose the visible region of the rendered page and
//! copy it into the window buffer (with HiDPI nearest-neighbour scaling).
//!
//! The worker renders the whole page as `0RGB` pixels in **CSS pixels**
//! (`frame_width x frame_height`). The window buffer is in **physical pixels**
//! (`dest_width x dest_height`) and shows `scroll_y..scroll_y + css_viewport`
//! CSS rows. All functions are pure and unit-tested without a display.

use crate::convert::WHITE_0RGB;

/// Fills `dest` with opaque white.
pub fn fill_white(dest: &mut [u32]) {
    dest.fill(WHITE_0RGB);
}

/// Copies the visible `view_width x view_height` region starting at row
/// `scroll_y` out of `frame` (1:1 pixels).
///
/// Rows/columns outside the frame are filled with opaque white so the caller
/// always receives a full buffer.
pub fn viewport_slice(
    frame: &[u32],
    frame_width: u32,
    frame_height: u32,
    scroll_y: u32,
    view_width: u32,
    view_height: u32,
) -> Vec<u32> {
    let len = match (view_width as usize).checked_mul(view_height as usize) {
        Some(len) if len <= crate::convert::MAX_FRAMEBUFFER_PIXELS => len,
        _ => return Vec::new(),
    };
    let mut out = vec![WHITE_0RGB; len];
    for row in 0..view_height {
        let source_row = u64::from(scroll_y) + u64::from(row);
        if source_row >= u64::from(frame_height) {
            continue;
        }
        let Some(source_offset) = (source_row as usize).checked_mul(frame_width as usize) else {
            continue;
        };
        let available = frame_width as usize;
        let copied = available.min(view_width as usize);
        let Some(source) = frame.get(source_offset..source_offset.saturating_add(copied)) else {
            continue;
        };
        let dest_offset = (row as usize) * (view_width as usize);
        if let Some(dest) = out.get_mut(dest_offset..dest_offset + copied) {
            dest.copy_from_slice(source);
        }
    }
    out
}

/// Physical-pixel height of the browser chrome band that must be reserved above the page
/// (F7-CHROME-OVERLAY / #1012). The band scales with HiDPI and is clamped so the page always
/// keeps at least one physical row. Pure so the wiring can be tested without a window.
pub fn chrome_band_pixels(scale: f64, dest_height: u32) -> u32 {
    chrome_band_pixels_for(scale, crate::chrome::CHROME_HEIGHT, dest_height)
}

/// Physical-pixel height of a chrome band with an arbitrary CSS height.
pub fn chrome_band_pixels_for(scale: f64, band_css_height: f32, dest_height: u32) -> u32 {
    ((f64::from(band_css_height) * scale).round().max(0.0) as u32)
        .min(dest_height.saturating_sub(1))
}

/// Physical-pixel width of the expanded vertical tab rail, clamped so the page keeps one column.
pub fn chrome_rail_pixels(scale: f64, dest_width: u32) -> u32 {
    chrome_rail_pixels_for(scale, crate::chrome::RAIL_EXPANDED_WIDTH, dest_width)
}

/// Physical-pixel width of a rail with an arbitrary CSS width, clamped to the window.
pub fn chrome_rail_pixels_for(scale: f64, rail_css_width: f32, dest_width: u32) -> u32 {
    ((f64::from(rail_css_width) * scale).round().max(0.0) as u32).min(dest_width.saturating_sub(1))
}

/// Presents `scroll_y..scroll_y + css_viewport_height` from the CSS-pixel frame
/// into the physical window buffer, scaling with nearest-neighbour sampling.
///
/// Returns `false` when the frame or the destination buffer is unusable (the
/// destination is then filled with white).
pub fn present_region(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
) -> bool {
    present_region_at(
        frame,
        scroll_y,
        css_viewport_height,
        0,
        dest,
        dest_width,
        dest_height,
    )
}

/// Like [`present_region`], but starts painting the page below `dest_y_offset`
/// physical rows (leaving rows `0..dest_y_offset` white for the browser chrome).
pub fn present_region_at(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    dest_y_offset: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
) -> bool {
    // Legacy path: the frame itself defines the source width (scale 1).
    let css_viewport_width = frame.map_or(0, |(_, width, _)| width);
    present_region_with_insets_scaled(
        frame,
        scroll_y,
        css_viewport_height,
        css_viewport_width,
        0,
        dest_y_offset,
        dest,
        dest_width,
        dest_height,
        1.0,
    )
}

/// Presents the page into the sub-rectangle left by browser-chrome insets.
///
/// Like [`present_region_at`], but also leaves columns `0..dest_x_offset` white for
/// the vertical tab rail and samples the page over `css_viewport_width` CSS columns
/// instead of the whole frame width. With `dest_x_offset == 0` and
/// `css_viewport_width == frame_width` the output is byte-identical to
/// [`present_region_at`].
// The flat signature is part of the Phase 1 presentation contract: callers pass
// frame geometry, viewport metrics and destination offsets without allocating.
#[allow(clippy::too_many_arguments)]
pub fn present_region_with_insets(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    css_viewport_width: u32,
    dest_x_offset: u32,
    dest_y_offset: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
) -> bool {
    present_region_with_insets_scaled(
        frame,
        scroll_y,
        css_viewport_height,
        css_viewport_width,
        dest_x_offset,
        dest_y_offset,
        dest,
        dest_width,
        dest_height,
        1.0,
    )
}

/// Presents a **physical-resolution** frame while `scroll_y` and
/// `css_viewport_height` remain CSS pixels. `raster_scale` converts CSS rows to
/// raster rows; the source columns come from the frame itself, so a window
/// whose buffer matches the frame becomes a 1:1 blit.
pub fn present_region_at_scaled(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    raster_scale: f64,
) -> bool {
    present_region_at_scaled_offset(
        frame,
        scroll_y,
        css_viewport_height,
        0,
        dest,
        dest_width,
        dest_height,
        raster_scale,
    )
}

/// Like [`present_region_at_scaled`], but reserves the first `dest_y_offset`
/// physical rows for browser chrome (F7-CHROME-OVERLAY / #1012). The page is
/// mapped into the remaining rows; the reserved rows are filled with white.
///
/// `u32::MAX` makes the source width resolve to the frame width: a physical
/// frame presented into a same-sized physical window is a 1:1 blit.
#[allow(clippy::too_many_arguments)]
pub fn present_region_at_scaled_offset(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    dest_y_offset: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    raster_scale: f64,
) -> bool {
    present_region_with_insets_scaled(
        frame,
        scroll_y,
        css_viewport_height,
        u32::MAX,
        0,
        dest_y_offset,
        dest,
        dest_width,
        dest_height,
        raster_scale,
    )
}

/// Like [`present_region_at_scaled`], with a CSS-width page inset (vertical rail).
/// The page samples `css_viewport_width * raster_scale` source columns.
#[allow(clippy::too_many_arguments)]
pub fn present_region_with_insets_scaled(
    frame: Option<(&[u32], u32, u32)>,
    scroll_y: u32,
    css_viewport_height: u32,
    css_viewport_width: u32,
    dest_x_offset: u32,
    dest_y_offset: u32,
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    raster_scale: f64,
) -> bool {
    let expected = match (dest_width as usize).checked_mul(dest_height as usize) {
        Some(expected) => expected,
        None => return false,
    };
    if dest.len() != expected || dest_width == 0 || dest_height == 0 {
        return false;
    }
    let Some((pixels, frame_width, frame_height)) = frame else {
        fill_white(dest);
        return false;
    };
    if frame_width == 0
        || frame_height == 0
        || pixels.len() < (frame_width as usize) * (frame_height as usize)
    {
        fill_white(dest);
        return false;
    }
    // Defensive: a hostile/host scale must not poison the row arithmetic.
    let scale = if raster_scale.is_finite() && raster_scale > 0.0 {
        raster_scale
    } else {
        1.0
    };
    let css_viewport_height = css_viewport_height.max(1);
    // CSS columns → source columns; at scale 1 this is the legacy width.
    let src_width = ((f64::from(css_viewport_width) * scale).round() as u64)
        .max(1)
        .min(u64::from(frame_width));
    let frame_width = frame_width as usize;
    let frame_height = frame_height as usize;
    let y_offset = (dest_y_offset as usize).min(dest_height as usize);
    let page_dest_height = (dest_height as usize - y_offset).max(1);
    let x_offset = (dest_x_offset as usize).min(dest_width as usize);
    let page_dest_width = (dest_width as usize - x_offset).max(1) as u64;
    for dest_row in 0..y_offset {
        let dest_offset = dest_row * dest_width as usize;
        if let Some(row_slice) = dest.get_mut(dest_offset..dest_offset + dest_width as usize) {
            row_slice.fill(WHITE_0RGB);
        }
    }
    for dest_row in y_offset..dest_height as usize {
        let page_row = dest_row - y_offset;
        // Keep the legacy integer-division result at scale 1 (goldens), but
        // retain the fractional CSS row for high-DPI upscaling: integer
        // division alone collapses every row when css_viewport_height <
        // page_dest_height (physical window > CSS viewport).
        let css_row = f64::from(scroll_y)
            + (page_row as f64 * f64::from(css_viewport_height)) / page_dest_height as f64;
        let dest_offset = dest_row * dest_width as usize;
        let Some(dest_row_slice) = dest.get_mut(dest_offset..dest_offset + dest_width as usize)
        else {
            return false;
        };
        // CSS scroll/viewport → physical raster row. This is the only place
        // where the CSS/raster boundary is crossed during presentation.
        let src_row = (css_row * scale).floor() as u64;
        if src_row >= frame_height as u64 {
            dest_row_slice.fill(WHITE_0RGB);
            continue;
        }
        dest_row_slice[..x_offset].fill(WHITE_0RGB);
        let source_offset = (src_row as usize) * frame_width;
        for (page_column, pixel) in dest_row_slice[x_offset..].iter_mut().enumerate() {
            let css_column = (page_column as u64 * src_width) / page_dest_width;
            *pixel = pixels[source_offset + css_column.min(frame_width as u64 - 1) as usize];
        }
    }
    true
}

#[cfg(test)]
mod unit_tests;
