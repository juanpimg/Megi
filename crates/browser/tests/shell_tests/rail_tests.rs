//! Headless tests for the vertical tab rail geometry and state transitions.
//!
//! The real window needs a winit `EventLoop`, so these tests exercise the same pure
//! helpers the window loop calls: rail hit-testing, tab activation and the page
//! viewport shrink caused by Ctrl+B's rail toggle.

use megi_browser::chrome::{self, RailHit};
use megi_browser::links::physical_to_page;
use megi_browser::tabs::TabsState;
use megi_browser::window::rail_state;

#[test]
fn rail_click_activates_tab_and_page_coordinates_are_shifted() {
    let mut tabs = TabsState::from_urls(vec![
        "https://a.example/".to_owned(),
        "https://b.example/".to_owned(),
    ]);
    assert_eq!(tabs.active_index(), 0, "first tab starts active");
    let labels: Vec<String> = tabs
        .tabs()
        .iter()
        .map(megi_browser::tabs::Tab::display_title)
        .collect();
    let view = rail_state::view(true, &labels, 0, None).expect("expanded rail");
    let layout = chrome::layout_rail(1024.0, 600.0, 1.0, &view);
    let row = &layout.tabs[1].rect;
    let hit = chrome::rail_hit(&layout, row.x + row.width / 2.0, row.y + row.height / 2.0);
    assert_eq!(hit, Some(RailHit::Activate(1)));
    let Some(RailHit::Activate(index)) = hit else {
        panic!("rail row must activate its tab");
    };
    assert!(tabs.activate(index), "rail index must be a live tab");
    assert_eq!(tabs.active_index(), 1, "rail click activates the tab");

    // Physical pointer X is shifted by the rail column before page conversion; the
    // closed rail must keep the historical mapping byte-for-byte.
    let scale = 2.0;
    let physical_x = 500.0;
    let open = physical_to_page(
        physical_x - rail_state::physical_offset(scale, true),
        0.0,
        scale,
        0,
    );
    let closed = physical_to_page(
        physical_x - rail_state::physical_offset(scale, false),
        0.0,
        scale,
        0,
    );
    assert_eq!(open.0, 82.0, "168 CSS px reserved at 2x = 336 physical px");
    assert_eq!(
        closed.0, 250.0,
        "closed rail leaves pointer mapping unchanged"
    );
    assert_eq!(open.0, rail_state::page_x(physical_x, scale, true) as f32);
    assert_eq!(
        closed.0,
        rail_state::page_x(physical_x, scale, false) as f32
    );
}

#[test]
fn ctrl_b_toggles_rail_and_reflows_page() {
    // Ctrl+B resolves to ShellAction::ToggleRail in the shortcut registry; the
    // handler flips this pure state and repaints at the new page width.
    let css_width = 1024u32;
    let closed = false;
    let open = rail_state::toggle(closed);
    assert!(open, "Ctrl+B opens the rail from the default closed state");
    assert_eq!(rail_state::css_width(closed), 0.0);
    assert_eq!(rail_state::css_width(open), chrome::RAIL_EXPANDED_WIDTH);

    let before = rail_state::page_viewport_width(css_width, closed);
    let after = rail_state::page_viewport_width(css_width, open);
    assert_eq!(before, css_width, "closed rail keeps the full page width");
    assert_eq!(
        before - after,
        chrome::RAIL_EXPANDED_WIDTH as u32,
        "open rail reflows the page by exactly one expanded rail"
    );
    assert_eq!(
        rail_state::page_viewport_width(css_width, rail_state::toggle(open)),
        before,
        "second Ctrl+B restores the original page width"
    );
}
