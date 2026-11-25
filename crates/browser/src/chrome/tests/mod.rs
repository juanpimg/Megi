//! Unit tests for the chrome module, organised by feature.

use crate::chrome::*;
use megi_webview::paint::Color;

mod address_bar;
mod dropdown;
mod menus;
mod paint;
mod rail;
mod tabs;
mod tokens;

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
        rail: None,
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

#[test]
fn native_widget_overlay_only_touches_its_patch_region() {
    // The overlay must not allocate or scan a full-framebuffer pixmap: pixels outside the
    // widget's expanded box must remain exactly as they were.
    let (width, height) = (400u32, 300u32);
    let mut dest = vec![0xDEAD_BEEFu32; (width * height) as usize];
    let rect = crate::links::CssRect::new(10.0, 10.0, 20.0, 20.0);
    let visual = crate::forms::widget_visual(
        crate::forms::FormControlKind::Checkbox,
        true,
        true,
        false,
        false,
        false,
    );
    super::draw_native_widget(&mut dest, width, height, 1.0, 0, rect, visual);
    let mut touched = 0usize;
    for (i, pixel) in dest.iter().enumerate() {
        if *pixel != 0xDEAD_BEEF {
            touched += 1;
            let x = (i as u32) % width;
            let y = (i as u32) / width;
            assert!(
                (6..34).contains(&x)
                    && (6 + crate::chrome::CHROME_HEIGHT as u32
                        ..34 + crate::chrome::CHROME_HEIGHT as u32)
                        .contains(&y),
                "overlay touched pixel outside its patch: ({x},{y})"
            );
        }
    }
    assert!(touched > 0, "overlay must paint something");
    assert!(
        touched < 2000,
        "a single widget must not touch thousands of pixels: {touched}"
    );
}

#[test]
fn native_widget_content_overlay_renders_text_and_caret_inside_local_patch() {
    let (width, height) = (400u32, 300u32);
    let mut blank = vec![0xDEAD_BEEFu32; (width * height) as usize];
    let mut typed = blank.clone();
    let rect = crate::links::CssRect::new(10.0, 10.0, 120.0, 22.0);
    let visual = crate::forms::widget_visual(
        crate::forms::FormControlKind::TextInput,
        false,
        true,
        false,
        false,
        false,
    );
    super::draw_native_widget(&mut blank, width, height, 1.0, 0, rect, visual);
    super::draw_native_widget_with_content(
        &mut typed,
        width,
        height,
        1.0,
        0,
        rect,
        visual,
        super::NativeWidgetContent {
            kind: crate::forms::FormControlKind::TextInput,
            text: "hello",
            checked: false,
            caret: Some(5),
            disabled: false,
        },
    );
    assert_ne!(
        blank, typed,
        "live typed text and caret must visibly paint into the local widget patch"
    );
    for (i, pixel) in typed.iter().enumerate() {
        if *pixel != 0xDEAD_BEEF {
            let x = (i as u32) % width;
            let y = (i as u32) / width;
            assert!(
                (6..134).contains(&x)
                    && (6 + crate::chrome::CHROME_HEIGHT as u32
                        ..36 + crate::chrome::CHROME_HEIGHT as u32)
                        .contains(&y),
                "content overlay touched pixel outside its patch: ({x},{y})"
            );
        }
    }
}
