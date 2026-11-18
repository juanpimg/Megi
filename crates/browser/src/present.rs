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
    let frame_width = frame_width as usize;
    let frame_height = frame_height as usize;
    let y_offset = (dest_y_offset as usize).min(dest_height as usize);
    let page_dest_height = (dest_height as usize - y_offset).max(1);
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
        let source_offset = (css_row as usize) * frame_width;
        for (column, pixel) in dest_row_slice.iter_mut().enumerate() {
            let css_column = column * frame_width / dest_width as usize;
            *pixel = pixels[source_offset + css_column.min(frame_width - 1)];
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
}
