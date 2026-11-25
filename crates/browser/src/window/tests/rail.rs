//! Rail wiring and pure rail geometry tests, split from `tests/mod.rs` to keep
//! that module under the 500-line architecture cap.

use super::shortcuts::{Modifiers, ShellAction, resolve_shortcut, toggle_rail};
use super::*;
use crate::chrome::{RAIL_COLLAPSED_WIDTH, RAIL_EXPANDED_WIDTH, RailHit, layout_rail, rail_hit};
use winit::keyboard::Key;

fn char_key(text: &str) -> Key {
    Key::Character(text.into())
}

#[test]
fn rail_wiring_ctrl_b_toggles_rail() {
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    assert_eq!(
        resolve_shortcut(ctrl, &char_key("B")),
        Some(ShellAction::ToggleRail)
    );
    let (open, expanded) = toggle_rail(false, false, ShellAction::ToggleRail);
    assert_eq!(
        (open, expanded),
        (true, true),
        "Ctrl+B opens the rail expanded"
    );
    let (open, expanded) = toggle_rail(open, expanded, ShellAction::ToggleRail);
    assert_eq!((open, expanded), (false, true), "Ctrl+B closes the rail");
    assert_eq!(
        toggle_rail(true, true, ShellAction::NewTab),
        (true, true),
        "unrelated actions leave the rail alone"
    );
}

#[test]
fn rail_wiring_click_tab_activates() {
    let labels = vec!["Alpha".to_owned(), "Beta".to_owned(), "Gamma".to_owned()];
    let view = rail_state::view_for(true, true, &labels, 0, None).expect("expanded rail");
    let layout = layout_rail(640.0, 400.0, 1.0, &view);
    let rect = layout.tabs[1].rect;
    assert_eq!(
        rail_hit(&layout, rect.x + 1.0, rect.y + 1.0),
        Some(RailHit::Activate(1))
    );
    let mut tabs = TabsState::from_urls(vec![tabs::BLANK_TAB_URL.to_owned(); 3]);
    assert!(tabs.activate(1), "the rail target tab exists");
    assert_eq!(tabs.active_index(), 1, "clicking the rail tab activates it");
}

#[test]
fn rail_wiring_expansion_aware_geometry() {
    // A closed rail reserves zero columns regardless of the expansion preference.
    assert_eq!(rail_state::css_width_for(false, false), 0.0);
    assert_eq!(rail_state::css_width_for(false, true), 0.0);
    assert_eq!(rail_state::physical_offset_for(2.0, false, true), 0.0);
    // The collapsed peek is narrow; the expanded rail shows full titles.
    assert_eq!(rail_state::css_width_for(true, false), RAIL_COLLAPSED_WIDTH);
    assert_eq!(rail_state::css_width_for(true, true), RAIL_EXPANDED_WIDTH);
    assert_eq!(
        rail_state::physical_offset_for(2.0, true, false),
        f64::from(RAIL_COLLAPSED_WIDTH) * 2.0
    );
    assert_eq!(
        rail_state::page_x_for(100.0, 1.0, true, false),
        100.0 - f64::from(RAIL_COLLAPSED_WIDTH),
        "collapsed rail inset is subtracted before scaling"
    );
    let labels = vec!["one".to_owned(), "two".to_owned()];
    let collapsed = rail_state::view_for(true, false, &labels, 1, None).expect("collapsed rail");
    assert!(!collapsed.expanded);
    assert_eq!(collapsed.width_css, RAIL_COLLAPSED_WIDTH);
    let expanded = rail_state::view_for(true, true, &labels, 1, None).expect("expanded rail");
    assert!(expanded.expanded);
    assert_eq!(expanded.width_css, RAIL_EXPANDED_WIDTH);
}

#[test]
fn rail_css_width_defaults_to_zero() {
    // The closed rail changes no pixel: WindowApp initializes rail_open=false
    // and the pure geometry helper must reserve zero CSS columns for it.
    assert_eq!(rail_state::css_width(false), 0.0);
    assert_eq!(
        rail_state::page_viewport_width(1024, false),
        1024,
        "closed rail leaves the page viewport untouched"
    );
}

#[test]
fn rail_view_none_when_closed() {
    let labels = vec!["one".to_owned(), "two".to_owned()];
    assert!(
        rail_state::view(false, &labels, 0, None).is_none(),
        "a closed rail must never be laid out or painted"
    );
    let open = rail_state::view(true, &labels, 1, None).expect("expanded rail");
    assert_eq!(open.labels, labels.as_slice());
    assert_eq!(open.active, 1);
    assert_eq!(open.width_css, rail_state::css_width(true));
    assert!(open.expanded);
}

#[test]
fn chrome_rail_pixels_track_rail_css_width_times_scale() {
    // `present_region_with_insets` reserves this many physical columns; the
    // value must follow rail_css_width() so a closed rail is a true no-op.
    let scale = 1.5;
    let dest_width = 800u32;
    let expected = (f64::from(rail_state::css_width(true)) * scale).round() as u32;
    assert_eq!(present::chrome_rail_pixels(scale, dest_width), expected);
    assert_eq!(
        present::chrome_rail_pixels_for(scale, rail_state::css_width(true), dest_width),
        expected
    );
    assert_eq!(
        present::chrome_rail_pixels_for(scale, rail_state::css_width(false), dest_width),
        0,
        "closed rail reserves no physical columns"
    );
}
