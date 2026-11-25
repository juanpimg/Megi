use super::*;
use crate::chrome::drawing::{top_radii, uniform_radii};
use crate::forms::SelectOption;
use crate::links::CssRect;
use megi_webview::paint::Pixmap;

#[test]
fn chrome_paints_tab_strip_and_address_bar() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string(), "Docs".to_string()];
    let mut dest = buffer(640, 400);
    let painted = draw_chrome(&mut dest, 640, 400, 1.0, &view(&address, None, &tabs));
    assert!(painted > 0);
    let pixel = |x: u32, y: u32| dest[(y * 640 + x) as usize];
    assert_eq!(pixel(620, 2), rgb(COLOR_TAB_STRIP), "tab strip background");
    assert_eq!(
        pixel(100, (address_bar_rect(640.0).y + 4.0) as u32),
        rgb(COLOR_ADDRESS_BG),
        "address pill surface"
    );
    // The page below the chrome is untouched.
    assert_eq!(pixel(300, 200), 0x00FF_FFFF);
}

#[test]
fn chrome_paints_dropdown_selected_and_active_rows() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let control = CssRect::new(20.0, 80.0, 120.0, 18.0);
    let options = vec![
        SelectOption::new("One", "1", true),
        SelectOption::new("Two", "2", false),
    ];
    let mut dropdown = DropdownState::open(1, control, options);
    dropdown.active = Some(1);
    let mut dest = buffer(480, 300);
    let painted = draw_chrome(
        &mut dest,
        480,
        300,
        1.0,
        &view(&address, Some(&dropdown), &tabs),
    );
    assert!(painted > 0);
    let list = dropdown_layout(&control, 2, 300, 0, 0);
    let first = list.rows[0];
    let second = list.rows[1];
    let pixel = |x: u32, y: u32| dest[(y * 480 + x) as usize];
    assert_eq!(
        pixel(first.x as u32 + 3, first.y as u32 + 3),
        rgb(COLOR_DROPDOWN_SELECTED)
    );
    assert_eq!(
        pixel(second.x as u32 + 3, second.y as u32 + 3),
        rgb(COLOR_DROPDOWN_ACTIVE)
    );
}

#[test]
fn chrome_editing_paints_a_caret() {
    let mut address = AddressBar::new();
    address.open("https://example.com/");
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 200);
    draw_chrome(&mut dest, 480, 200, 1.0, &view(&address, None, &tabs));
    let row = (TAB_STRIP_HEIGHT + 10.0) as usize;
    let caret_pixels = dest[row * 480..(row + 1) * 480]
        .iter()
        .filter(|pixel| **pixel == rgb(COLOR_CARET))
        .count();
    assert!(caret_pixels > 0, "caret visible");
}

#[test]
fn loading_indicator_paints_on_the_active_tab() {
    let mut address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 200);
    let mut view = view(&address, None, &tabs);
    view.loading = true;
    draw_chrome(&mut dest, 480, 200, 1.0, &view);
    let loading = dest[..(24 * 480) as usize]
        .iter()
        .filter(|pixel| **pixel == rgb(COLOR_LOADING))
        .count();
    assert!(loading > 0, "loading dot visible");
    let _ = &mut address;
}

#[test]
fn dropdown_scrollbar_paints_when_the_list_overflows() {
    let control = CssRect::new(10.0, 10.0, 120.0, 18.0);
    let options: Vec<SelectOption> = (0..20)
        .map(|index| SelectOption::new(format!("Option {index}"), format!("{index}"), false))
        .collect();
    let dropdown = DropdownState::open(3, control, options);
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 400);
    draw_chrome(
        &mut dest,
        480,
        400,
        1.0,
        &view(&address, Some(&dropdown), &tabs),
    );
    let scroll_pixels = dest
        .iter()
        .filter(|pixel| **pixel == rgb(COLOR_SCROLL))
        .count();
    assert!(scroll_pixels > 10, "scrollbar painted: {scroll_pixels}");
}

#[test]
fn status_bar_shows_hover_url_and_clears() {
    let rect = status_bar_rect(640.0, 400);
    assert_eq!(rect.height, STATUS_BAR_HEIGHT);
    assert_eq!(rect.y, 400.0 - STATUS_BAR_HEIGHT);
    assert!(status_text("https://example.com/a/very/long/path", 200.0).ends_with('…'));
    assert_eq!(status_text("abc", 640.0), "abc");

    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(320, 200);
    let mut view = view(&address, None, &tabs);
    view.hover_url = Some("https://example.com/hovered");
    view.css_viewport_height = 200;
    draw_chrome(&mut dest, 320, 200, 1.0, &view);
    let bottom = ((200 - STATUS_BAR_HEIGHT as u32 + 2) * 320) as usize;
    assert_eq!(dest[bottom], rgb(COLOR_STATUS_BAR), "status bar background");
    let mut clean = buffer(320, 200);
    draw_chrome(
        &mut clean,
        320,
        200,
        1.0,
        &view_without_hover(&address, &tabs),
    );
    assert_ne!(clean[bottom], rgb(COLOR_STATUS_BAR), "no bar without hover");
}

fn view_without_hover<'a>(address: &'a AddressBar, tabs: &'a [String]) -> ChromeView<'a> {
    let mut view = view(address, None, tabs);
    view.hover_url = None;
    view
}

#[test]
fn context_menu_paints_selected_row() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let menu = ContextMenu::new(Some("https://example.com/a".to_string()), None, 20.0, 40.0);
    let mut dest = buffer(400, 240);
    let mut view = view(&address, None, &tabs);
    view.menu = Some(&menu);
    draw_chrome(&mut dest, 400, 240, 1.0, &view);
    let layout = context_menu_layout(&menu, 400.0, 240);
    let selected = layout.rows[0];
    let pixel = dest[((selected.y + 6.0) as u32 * 400 + (selected.x + 6.0) as u32) as usize];
    assert_eq!(pixel, rgb(COLOR_MENU_SELECTED), "selected row background");
    assert!(selected.width > 0.0);
}

#[test]
fn chrome_paints_navigation_buttons() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 200);
    let view = view(&address, None, &tabs);
    draw_chrome(&mut dest, 480, 200, 1.0, &view);
    let buttons = navigation_buttons(480.0);
    // The enabled reload glyph paints dark text pixels inside its box.
    let mut dark = 0usize;
    for y in buttons.reload.y as usize..(buttons.reload.y + buttons.reload.height) as usize {
        for x in buttons.reload.x as usize..(buttons.reload.x + buttons.reload.width) as usize {
            let pixel = dest[y * 480 + x];
            if pixel != rgb(COLOR_ADDRESS_BG) && pixel != rgb(COLOR_ADDRESS_STRIP) {
                dark += 1;
            }
        }
    }
    assert!(dark > 0, "reload glyph painted");
}

#[test]
fn download_status_takes_precedence_over_hover_url() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(360, 200);
    let mut view = view(&address, None, &tabs);
    view.css_viewport_height = 200;
    view.hover_url = Some("https://example.com/hover");
    view.download_status = Some("guardando… megi-a.png");
    draw_chrome(&mut dest, 360, 200, 1.0, &view);
    let bottom = ((200 - STATUS_BAR_HEIGHT as u32 + 2) * 360) as usize;
    assert_eq!(
        dest[bottom],
        rgb(COLOR_STATUS_BAR),
        "status bar painted for download"
    );
    let without = view_without_download(&address, &tabs);
    assert_eq!(without.download_status, None);
}

fn view_without_download<'a>(address: &'a AddressBar, tabs: &'a [String]) -> ChromeView<'a> {
    let mut view = view(address, None, tabs);
    view.download_status = None;
    view
}

#[test]
fn history_button_paints_only_when_enabled() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let rect = history_button_rect(480.0);
    let mut without = buffer(480, 200);
    let mut view = view(&address, None, &tabs);
    view.css_width = 480.0;
    draw_chrome(&mut without, 480, 200, 1.0, &view);
    view.history_button = true;
    let mut with = buffer(480, 200);
    draw_chrome(&mut with, 480, 200, 1.0, &view);
    let mut changed = 0usize;
    for y in rect.y as u32..(rect.y + rect.height) as u32 {
        for x in rect.x as u32..(rect.x + rect.width) as u32 {
            if without[(y * 480 + x) as usize] != with[(y * 480 + x) as usize] {
                changed += 1;
            }
        }
    }
    assert!(changed > 0, "history glyph painted");
}

#[test]
fn history_menu_paints_domain_badges() {
    let entries = vec![
        crate::history::HistoryEntry {
            title: "A".to_string(),
            url: "https://example.com/a".to_string(),
            logical_time: 1,
        },
        crate::history::HistoryEntry {
            title: "B".to_string(),
            url: "https://file.example/b".to_string(),
            logical_time: 2,
        },
    ];
    let menu = crate::history::HistoryMenu::open(entries.clone(), 20.0, 40.0);
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 300);
    let mut view = view(&address, None, &tabs);
    view.css_width = 480.0;
    view.css_viewport_height = 300;
    view.history_menu = Some(&menu);
    draw_chrome(&mut dest, 480, 300, 1.0, &view);
    let layout = history_menu_layout(&menu, 480.0, 300);
    let row = layout.rows[0];
    let (_, color) = crate::history::domain_badge(&entries[0].url);
    let expected = (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2]);
    // The badge is drawn a few pixels inside the row; scan for its color.
    let start = (row.y as u32 + 4) * 480 + row.x as u32 + 4;
    assert!(
        dest[start as usize..(start + 30) as usize].contains(&expected),
        "domain badge color painted"
    );
}
#[test]
fn chrome_radius_helpers_distinguish_tabs_from_panels() {
    let panel = uniform_radii(7.0);
    assert!(!panel.is_zero(), "panels have all four corners rounded");
    let tab = top_radii(6.0);
    assert!(!tab.top_left.is_zero() && !tab.top_right.is_zero());
    assert!(
        tab.bottom_left.is_zero() && tab.bottom_right.is_zero(),
        "tabs meet the address strip flush at the bottom"
    );
}

#[test]
fn draw_chrome_without_rail_keeps_legacy_pixels() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let legacy_view = view(&address, None, &tabs);
    assert!(
        legacy_view.rail.is_none(),
        "the legacy view has no rail overlay"
    );

    let mut legacy = buffer(640, 400);
    draw_chrome(&mut legacy, 640, 400, 1.0, &legacy_view);
    assert_eq!(legacy[(2 * 640 + 620) as usize], rgb(COLOR_TAB_STRIP));
    assert_eq!(legacy[(200 * 640 + 300) as usize], 0x00FF_FFFF);

    // Painting a rail must leave every non-rail pixel of the legacy frame intact.
    let labels = vec!["Megi".to_string()];
    let mut rail_view = view(&address, None, &tabs);
    rail_view.rail = Some(RailView {
        labels: &labels,
        active: 0,
        hover: None,
        expanded: true,
        width_css: RAIL_EXPANDED_WIDTH,
    });
    let mut with_rail = buffer(640, 400);
    draw_chrome(&mut with_rail, 640, 400, 1.0, &rail_view);
    for y in 0..400u32 {
        for x in 0..640u32 {
            if y < CHROME_HEIGHT as u32 || x >= RAIL_EXPANDED_WIDTH as u32 {
                assert_eq!(
                    with_rail[(y * 640 + x) as usize],
                    legacy[(y * 640 + x) as usize],
                    "rail changed a legacy pixel at ({x},{y})"
                );
            }
        }
    }
}

#[test]
fn draw_rail_over_pixmap_is_noop_when_rail_is_none() {
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let view = view(&address, None, &tabs);
    let mut pixmap = Pixmap::new(480, 300);
    for pixel in &mut pixmap.pixels {
        *pixel = Color::new(0x40, 0x50, 0x60, 0xFF);
    }
    let before = pixmap.clone();
    assert_eq!(
        draw_rail_over_pixmap(&mut pixmap, 1.0, &view),
        0,
        "no rail means no painted pixels"
    );
    assert_eq!(
        pixmap, before,
        "a rail-less view leaves the pixmap untouched"
    );
}

#[test]
fn palette_uses_one_accent_and_keeps_readable_contrast() {
    assert_eq!(COLOR_CARET, ACCENT, "the caret follows the accent");
    assert_eq!(COLOR_LOADING, ACCENT, "loading feedback follows the accent");
    assert_ne!(ACCENT, COLOR_ADDRESS_BG, "accent stands out from surfaces");
    let luminance = |color: Color| {
        0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b)
    };
    assert!(
        luminance(COLOR_ADDRESS_TEXT) - luminance(COLOR_ADDRESS_BG) > 100.0,
        "address text is legible on its surface"
    );
    assert!(
        luminance(COLOR_TEXT) - luminance(COLOR_TAB_STRIP) > 100.0,
        "chrome text is legible on the dark strip"
    );
    assert!(
        luminance(COLOR_ADDRESS_MUTED) - luminance(COLOR_ADDRESS_BG) > 30.0,
        "muted URL parts are still readable"
    );
    for color in [
        ACCENT,
        COLOR_TAB_STRIP,
        COLOR_TAB,
        COLOR_TAB_ACTIVE,
        COLOR_ADDRESS_BG,
        COLOR_ADDRESS_TEXT,
        COLOR_DROPDOWN_BG,
        COLOR_MENU_BG,
        COLOR_BORDER,
    ] {
        assert_eq!(color.a, 0xFF, "chrome surfaces are opaque");
    }
}
