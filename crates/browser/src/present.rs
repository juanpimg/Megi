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
    let css_height = crate::chrome::CHROME_HEIGHT;
    ((f64::from(css_height) * scale).round() as u32).min(dest_height.saturating_sub(1))
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
    let css_viewport_width = frame.map_or(0, |(_, width, _)| width);
    present_region_with_insets(
        frame,
        scroll_y,
        css_viewport_height,
        css_viewport_width,
        0,
        dest_y_offset,
        dest,
        dest_width,
        dest_height,
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
    let css_viewport_height = css_viewport_height.max(1);
    let src_width = u64::from(css_viewport_width.max(1).min(frame_width));
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
        let css_row = u64::from(scroll_y)
            + (page_row as u64 * u64::from(css_viewport_height)) / page_dest_height as u64;
        let dest_offset = dest_row * dest_width as usize;
        let Some(dest_row_slice) = dest.get_mut(dest_offset..dest_offset + dest_width as usize)
        else {
            return false;
        };
        if css_row >= frame_height as u64 {
            dest_row_slice.fill(WHITE_0RGB);
            continue;
        }
        dest_row_slice[..x_offset].fill(WHITE_0RGB);
        let source_offset = (css_row as usize) * frame_width;
        for (page_column, pixel) in dest_row_slice[x_offset..].iter_mut().enumerate() {
            let css_column = (page_column as u64 * src_width) / page_dest_width;
            *pixel = pixels[source_offset + css_column.min(frame_width as u64 - 1) as usize];
        }
    }
    true
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::convert::WHITE_0RGB;

    const RED: u32 = 0x00FF_0000;
    const GREEN: u32 = 0x0000_FF00;
    const BLUE: u32 = 0x0000_00FF;
    const YELLOW: u32 = 0x00FF_FF00;

    fn four_row_frame() -> Vec<u32> {
        // 2x4 frame: row 0 red, row 1 green, row 2 blue, row 3 yellow.
        vec![RED, RED, GREEN, GREEN, BLUE, BLUE, YELLOW, YELLOW]
    }

    #[test]
    fn viewport_slice_selects_the_scrolled_rows() {
        let frame = four_row_frame();
        let slice = viewport_slice(&frame, 2, 4, 1, 2, 2);
        assert_eq!(slice, vec![GREEN, GREEN, BLUE, BLUE]);
        let top = viewport_slice(&frame, 2, 4, 0, 2, 1);
        assert_eq!(top, vec![RED, RED]);
        let bottom = viewport_slice(&frame, 2, 4, 3, 2, 1);
        assert_eq!(bottom, vec![YELLOW, YELLOW]);
    }

    #[test]
    fn viewport_slice_pads_out_of_range_rows_with_white() {
        let frame = four_row_frame();
        let slice = viewport_slice(&frame, 2, 4, 3, 2, 3);
        assert_eq!(
            slice,
            vec![
                YELLOW, YELLOW, WHITE_0RGB, WHITE_0RGB, WHITE_0RGB, WHITE_0RGB
            ]
        );
        assert!(viewport_slice(&frame, 2, 4, 0, 0, 2).is_empty());
    }

    #[test]
    fn present_region_is_identity_at_scale_one() {
        let frame = four_row_frame();
        let mut dest = vec![0u32; 4];
        assert!(present_region(Some((&frame, 2, 4)), 1, 2, &mut dest, 2, 2));
        assert_eq!(dest, vec![GREEN, GREEN, BLUE, BLUE]);
    }

    #[test]
    fn present_region_scales_nearest_neighbour_for_hidpi() {
        // 2x2 CSS frame: top row red, bottom row blue; scale 2 => 4x4 physical.
        let frame = vec![RED, RED, BLUE, BLUE];
        let mut dest = vec![0u32; 16];
        assert!(present_region(Some((&frame, 2, 2)), 0, 2, &mut dest, 4, 4));
        assert_eq!(&dest[0..4], &[RED, RED, RED, RED]);
        assert_eq!(&dest[4..8], &[RED, RED, RED, RED]);
        assert_eq!(&dest[8..12], &[BLUE, BLUE, BLUE, BLUE]);
        assert_eq!(&dest[12..16], &[BLUE, BLUE, BLUE, BLUE]);
    }

    #[test]
    fn present_region_applies_scroll_before_scaling() {
        // 2x4 CSS frame; scroll 1 CSS row and scale 2 => rows green/blue.
        let frame = four_row_frame();
        let mut dest = vec![0u32; 8]; // 2x4 physical
        assert!(present_region(Some((&frame, 2, 4)), 1, 2, &mut dest, 2, 4));
        assert_eq!(
            dest,
            vec![GREEN, GREEN, GREEN, GREEN, BLUE, BLUE, BLUE, BLUE]
        );
    }

    #[test]
    fn present_region_falls_back_to_white() {
        let mut dest = vec![RED; 4];
        assert!(!present_region(None, 0, 2, &mut dest, 2, 2));
        assert!(dest.iter().all(|&pixel| pixel == WHITE_0RGB));

        let tiny = [RED];
        assert!(!present_region(Some((&tiny, 2, 2)), 0, 2, &mut dest, 2, 2));
        assert!(dest.iter().all(|&pixel| pixel == WHITE_0RGB));

        // Destination length mismatch is a hard error.
        let frame = four_row_frame();
        let mut short = vec![0u32; 3];
        assert!(!present_region(
            Some((&frame, 2, 4)),
            0,
            2,
            &mut short,
            2,
            2
        ));
    }

    #[test]
    fn chrome_band_pixels_scales_and_clamps_to_the_window() {
        // F7-CHROME-OVERLAY / #1012: the page must start below the chrome band at scroll_y=0.
        // The band scales with HiDPI and is clamped so the page always keeps at least 1 row.
        use crate::chrome::CHROME_HEIGHT;
        assert_eq!(chrome_band_pixels(1.0, 800), CHROME_HEIGHT as u32);
        assert_eq!(chrome_band_pixels(2.0, 800), (CHROME_HEIGHT * 2.0) as u32);
        assert_eq!(
            chrome_band_pixels(1.0, 30),
            29,
            "band clamped, page keeps 1 row"
        );
        assert_eq!(
            chrome_band_pixels(1.0, 1),
            0,
            "zero-height band for a 1px window"
        );
        assert_eq!(
            chrome_band_pixels(1.0, 0),
            0,
            "no band for a zero-height window"
        );
    }

    #[test]
    fn present_region_at_offsets_page_below_chrome_band() {
        let frame = vec![RED, RED, GREEN, GREEN];
        let mut dest = vec![0u32; 2 * 4];
        assert!(present_region_at(
            Some((&frame, 2, 2)),
            0,
            2,
            2,
            &mut dest,
            2,
            4
        ));
        assert_eq!(
            dest,
            vec![
                WHITE_0RGB, WHITE_0RGB, WHITE_0RGB, WHITE_0RGB, RED, RED, GREEN, GREEN
            ]
        );
    }

    #[test]
    fn present_region_with_insets_scales_into_the_remaining_columns() {
        // 4x2 CSS frame with four distinct columns; two physical inset columns on the left.
        const C0: u32 = 0x00_00_00_01;
        const C1: u32 = 0x00_00_00_02;
        const C2: u32 = 0x00_00_00_03;
        const C3: u32 = 0x00_00_00_04;
        let frame = [C0, C1, C2, C3, C0, C1, C2, C3];
        let mut dest = vec![0u32; 8 * 4];
        assert!(present_region_with_insets(
            Some((&frame, 4, 2)),
            0,
            2,
            4,
            2,
            0,
            &mut dest,
            8,
            4
        ));
        for row in dest.as_chunks::<8>().0 {
            assert_eq!(row, &[WHITE_0RGB, WHITE_0RGB, C0, C0, C1, C2, C2, C3]);
        }
    }

    #[test]
    fn present_region_with_insets_uses_the_css_viewport_width_for_sampling() {
        // Same frame/inset, but the visible page is only 2 CSS columns wide: samples
        // are spread over 2 columns instead of the frame's 4.
        const C0: u32 = 0x00_00_00_01;
        const C1: u32 = 0x00_00_00_02;
        const C2: u32 = 0x00_00_00_03;
        const C3: u32 = 0x00_00_00_04;
        let frame = [C0, C1, C2, C3, C0, C1, C2, C3];
        let mut dest = vec![0u32; 8 * 2];
        assert!(present_region_with_insets(
            Some((&frame, 4, 2)),
            0,
            2,
            2,
            2,
            0,
            &mut dest,
            8,
            2
        ));
        for row in dest.as_chunks::<8>().0 {
            assert_eq!(row, &[WHITE_0RGB, WHITE_0RGB, C0, C0, C0, C1, C1, C1]);
        }
    }

    #[test]
    fn present_region_with_insets_leaves_a_full_inset_entirely_white() {
        const C0: u32 = 0x00_00_00_01;
        let frame = [C0, C0, C0, C0, C0, C0, C0, C0];
        let mut dest = vec![RED; 8 * 2];
        for x_offset in [8, 20] {
            dest.fill(RED);
            assert!(present_region_with_insets(
                Some((&frame, 4, 2)),
                0,
                2,
                4,
                x_offset,
                0,
                &mut dest,
                8,
                2
            ));
            assert!(
                dest.iter().all(|&pixel| pixel == WHITE_0RGB),
                "x_offset {x_offset} leaves no page columns"
            );
        }
    }

    #[test]
    fn present_region_with_insets_matches_present_region_at_without_insets() {
        // With no horizontal inset and the full frame width, the new path must be
        // byte-for-byte the old one for every scroll/chrome-band combination.
        let frame = four_row_frame();
        for scroll_y in [0, 1] {
            for dest_y_offset in [0, 1] {
                let mut via_at = vec![0u32; 4 * 6];
                let mut via_insets = vec![0u32; 4 * 6];
                assert!(present_region_at(
                    Some((&frame, 2, 4)),
                    scroll_y,
                    2,
                    dest_y_offset,
                    &mut via_at,
                    4,
                    6
                ));
                assert!(present_region_with_insets(
                    Some((&frame, 2, 4)),
                    scroll_y,
                    2,
                    2,
                    0,
                    dest_y_offset,
                    &mut via_insets,
                    4,
                    6
                ));
                assert_eq!(
                    via_insets, via_at,
                    "scroll {scroll_y}, chrome band {dest_y_offset}"
                );
            }
        }
    }

    #[test]
    fn chrome_rail_pixels_scales_and_clamps_to_the_window() {
        use crate::chrome::{RAIL_COLLAPSED_WIDTH, RAIL_EXPANDED_WIDTH};
        assert_eq!(chrome_rail_pixels(1.0, 800), RAIL_EXPANDED_WIDTH as u32);
        assert_eq!(
            chrome_rail_pixels(2.0, 800),
            (RAIL_EXPANDED_WIDTH * 2.0) as u32
        );
        assert_eq!(
            chrome_rail_pixels(1.0, 5),
            4,
            "page keeps 1 physical column"
        );
        assert_eq!(chrome_rail_pixels(1.0, 1), 0);
        assert_eq!(chrome_rail_pixels(1.0, 0), 0);
        assert_eq!(chrome_rail_pixels_for(1.0, RAIL_COLLAPSED_WIDTH, 800), 12);
        assert_eq!(chrome_rail_pixels_for(1.0, -4.0, 800), 0);
    }
}
