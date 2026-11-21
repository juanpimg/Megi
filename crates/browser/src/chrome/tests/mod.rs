//! Unit tests for the chrome module, organised by feature.

use crate::chrome::*;
use megi_webview::paint::Color;

mod address_bar;
mod dropdown;
mod menus;
mod paint;
mod tabs;

fn view<'a>(
    address: &'a AddressBar,
    dropdown: Option<&'a DropdownState>,
    tabs: &'a [String],
) -> ChromeView<'a> {
    ChromeView {
        favicons: &[],
        tabs,
        active_tab: 0,
        address,
        fallback_url: "https://example.com/",
        dropdown,
        scroll_y: 0,
        css_viewport_height: 400,
        css_width: 640.0,
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
    }
}

fn buffer(width: u32, height: u32) -> Vec<u32> {
    vec![0x00FF_FFFF; (width * height) as usize]
}

/// Packs a palette token into the `0RGB` word used by the framebuffer tests.
/// Assertions reference tokens so a palette refresh cannot leave stale hex behind.
fn rgb(color: Color) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

#[test]
fn save_as_dialog_paints_a_centered_panel() {
    // F7-SAVE-AS / #988: the dialog must be a visible overlay, not an invisible state.
    let mut dest = vec![0u32; 400 * 200];
    draw_save_as_dialog(&mut dest, 400, 200, 1.0, "report.html", "HTML");
    let painted = dest.iter().filter(|&&pixel| pixel != 0).count();
    assert!(
        painted > 1000,
        "the dialog panel must paint visible pixels: {painted}"
    );
    // The top-left corner stays untouched (the panel is centered, not full-screen).
    assert_eq!(
        dest[0], 0,
        "dialog must not overwrite the whole framebuffer"
    );
}
