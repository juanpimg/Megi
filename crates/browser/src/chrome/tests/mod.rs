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
