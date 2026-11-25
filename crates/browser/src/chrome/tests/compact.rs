//! Phase 2 — compact chrome band and collapsed-rail hover peek.
//!
//! The band shrinks from 58 to 40 CSS pixels by hiding (not shrinking) the tab
//! strip; the address bar keeps its 28 px hit target. Peek widens the rail only
//! while hovered and jumps straight to the final width under reduced motion.

use super::*;
use crate::chrome::compact::{
    MIN_HIT_TARGET_CSS, PEEK_ANIMATION_MS, band_css_height, band_pixels, peek_labels_visible,
    peek_width_css,
};

#[test]
fn compact_band_is_40px_and_keeps_every_hit_target_at_least_22px() {
    assert_eq!(CHROME_HEIGHT, 58.0);
    assert_eq!(COMPACT_CHROME_HEIGHT, 40.0);
    assert_eq!(band_css_height(false), CHROME_HEIGHT);
    assert_eq!(band_css_height(true), COMPACT_CHROME_HEIGHT);
    const {
        assert!(
            ADDRESS_BAR_HEIGHT >= MIN_HIT_TARGET_CSS,
            "address bar must stay clickable in compact mode"
        )
    };
    const {
        assert!(
            NAV_BUTTON_SIZE >= MIN_HIT_TARGET_CSS,
            "navigation buttons must stay clickable in compact mode"
        )
    };
    const { assert!(MIN_HIT_TARGET_CSS >= 22.0) };
}

#[test]
fn compact_band_pixels_follow_scale_and_clamp_to_frame() {
    assert_eq!(band_pixels(false, 1.0, 600), 58);
    assert_eq!(band_pixels(true, 1.0, 600), 40);
    assert_eq!(band_pixels(true, 2.0, 600), 80);
    // Same clamp as `present::chrome_band_pixels`: the page keeps at least one row.
    assert_eq!(band_pixels(true, 2.0, 30), 29);
}

#[test]
fn compact_paint_shifts_address_rows_and_drops_tab_strip() {
    let address = AddressBar::new();
    let tabs = vec!["alpha".to_string()];
    let mut compact_view = view(&address, None, &tabs);
    compact_view.compact = true;
    let (width, height) = (640u32, 200u32);
    let mut legacy_dest = buffer(width, height);
    let mut compact_dest = buffer(width, height);
    crate::chrome::draw_chrome(
        &mut legacy_dest,
        width,
        height,
        1.0,
        &view(&address, None, &tabs),
    );
    crate::chrome::draw_chrome(&mut compact_dest, width, height, 1.0, &compact_view);
    // "Hide before shrink": the first 12 rows are the hidden tab strip and keep
    // their page pixels; the address strip (legacy 30..58) moves to rows 12..40.
    let shift = (CHROME_HEIGHT - COMPACT_CHROME_HEIGHT) as usize;
    let hidden_rows = (TAB_STRIP_HEIGHT - CHROME_HEIGHT + COMPACT_CHROME_HEIGHT) as usize;
    for y in 0..hidden_rows {
        for x in 0..width as usize {
            assert_eq!(
                compact_dest[y * width as usize + x],
                0x00FF_FFFF,
                "hidden tab strip must not repaint page pixels at ({x},{y})"
            );
        }
    }
    // Compare the address pill and buttons, skipping the strip boundary row where
    // the legacy tab strip antialiases its bottom edge.
    for y in (hidden_rows + 2)..COMPACT_CHROME_HEIGHT as usize {
        for x in 0..width as usize {
            assert_eq!(
                compact_dest[y * width as usize + x],
                legacy_dest[(y + shift) * width as usize + x],
                "compact pixel mismatch at ({x},{y})"
            );
        }
    }
}

#[test]
fn hover_peek_is_transient_and_reduced_motion_skips_the_animation() {
    assert_eq!(peek_width_css(false, false, 0, false), RAIL_COLLAPSED_WIDTH);
    assert_eq!(
        peek_width_css(false, true, PEEK_ANIMATION_MS, false),
        RAIL_EXPANDED_WIDTH
    );
    assert_eq!(
        peek_width_css(true, false, PEEK_ANIMATION_MS, false),
        RAIL_PEEK_WIDTH
    );
    let mid = peek_width_css(true, false, PEEK_ANIMATION_MS / 2, false);
    assert!(
        mid > RAIL_COLLAPSED_WIDTH && mid < RAIL_PEEK_WIDTH,
        "peek must interpolate while hovered: {mid}"
    );
    assert_eq!(
        peek_width_css(true, false, 0, true),
        RAIL_PEEK_WIDTH,
        "prefers-reduced-motion shows labels immediately"
    );
    assert!(!peek_labels_visible(true, false, 0, false));
    assert!(peek_labels_visible(true, false, 0, true));
    assert!(peek_labels_visible(true, false, PEEK_ANIMATION_MS, false));
    assert!(peek_labels_visible(false, true, 0, false));
}
