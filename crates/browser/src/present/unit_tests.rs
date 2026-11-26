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
fn dpr_raster_present_maps_css_scroll_to_physical_rows_and_blits_columns_1_to_1() {
    // Physical 1600x900 frame, CSS viewport 1280x720 at 1.25x: the window
    // buffer is the same size as the frame, so every column is a 1:1 blit
    // and CSS scroll row 100 addresses physical raster row 125.
    const W: u32 = 1600;
    const H: u32 = 900;
    let rows: Vec<u32> = (0..H).flat_map(|row| vec![row; W as usize]).collect();
    let mut dest = vec![0u32; (W * H) as usize];
    assert!(present_region_at_scaled(
        Some((&rows, W, H)),
        100,
        720,
        &mut dest,
        W,
        H,
        1.25
    ));
    for row in 0..H {
        let expected = if 125 + row < H { 125 + row } else { WHITE_0RGB };
        assert_eq!(
            dest[row as usize * W as usize],
            expected,
            "physical row {row} must sample CSS scroll 100 → raster {}",
            125 + row
        );
    }

    let columns: Vec<u32> = (0..H).flat_map(|_| (0..W).collect::<Vec<u32>>()).collect();
    let mut dest2 = vec![0u32; (W * H) as usize];
    assert!(present_region_at_scaled(
        Some((&columns, W, H)),
        0,
        720,
        &mut dest2,
        W,
        H,
        1.25
    ));
    for col in [0u32, 17, 799, 1599] {
        assert_eq!(
            dest2[col as usize], col,
            "physical column {col} must be copied 1:1"
        );
    }
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
fn present_region_at_scaled_offset_starts_the_physical_page_below_the_band() {
    // The frame is already at physical resolution (raster_scale 1.25), so the
    // page must begin exactly `dest_y_offset` physical rows down, with the
    // reserved rows left white for the chrome band.
    const W: u32 = 200;
    const H: u32 = 200;
    const OFFSET: u32 = 8;
    let frame: Vec<u32> = (0..H).flat_map(|row| vec![row; W as usize]).collect();
    let mut dest = vec![0u32; (W * H) as usize];
    assert!(present_region_at_scaled_offset(
        Some((&frame, W, H)),
        0,
        100,
        OFFSET,
        &mut dest,
        W,
        H,
        1.25
    ));
    for row in 0..OFFSET {
        assert!(
            dest[row as usize * W as usize..(row as usize + 1) * W as usize]
                .iter()
                .all(|&pixel| pixel == WHITE_0RGB),
            "physical row {row} is reserved for chrome"
        );
    }
    let page_rows = (H - OFFSET) as f64;
    for row in [OFFSET, OFFSET + 1, OFFSET + 2, OFFSET + 3, OFFSET + 10] {
        let page_row = row - OFFSET;
        let expected = (f64::from(page_row) * 100.0 / page_rows * 1.25).floor() as u32;
        assert_eq!(
            dest[row as usize * W as usize],
            expected,
            "physical row {row} samples physical raster row {expected}"
        );
    }
    // The legacy entry point is byte-identical to the offset variant with 0.
    let mut legacy = vec![0u32; (W * H) as usize];
    assert!(present_region_at_scaled(
        Some((&frame, W, H)),
        0,
        100,
        &mut legacy,
        W,
        H,
        1.25
    ));
    assert_eq!(legacy, {
        let mut zero = vec![0u32; (W * H) as usize];
        assert!(present_region_at_scaled_offset(
            Some((&frame, W, H)),
            0,
            100,
            0,
            &mut zero,
            W,
            H,
            1.25
        ));
        zero
    });
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
