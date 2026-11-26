//! Unit tests for the chrome module, organised by feature.

use crate::chrome::*;
use megi_webview::layout::Rect;
use megi_webview::paint::Color;

mod address_bar;
mod compact;
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
        compact: false,
        rail: None,
        shell: None,
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

use megi_webview::paint::Pixmap;

fn shell_view<'a>(labels: &'a [String], url: &'a str, top: f32) -> ShellView<'a> {
    ShellView {
        labels,
        favicons: &[],
        url,
        can_go_back: false,
        can_go_forward: false,
        active: 0,
        top_css: top,
        sidebar: true,
        hover: None,
        focus: None,
    }
}

#[test]
fn shell_hi_dpi_controls_share_one_row() {
    let labels = vec!["A".to_owned()];
    let view = shell_view(&labels, "https://example.com", CHROME_HEIGHT);
    let layout = layout_shell(1000.0, 700.0, 1.25, &view);
    let cy = |r: Rect| r.y + r.height * 0.5;
    assert!(
        (cy(layout.dots[0]) - cy(layout.nav.back)).abs() <= 6.0,
        "navigation must stay on the traffic-light row at hiDPI"
    );
    assert!(
        (cy(layout.toggle) - cy(layout.actions[0])).abs() <= 6.0,
        "toggle and right actions must stay on one row at hiDPI"
    );
}

#[test]
fn shell_narrow_sidebar_never_clips() {
    let labels = vec!["A".to_owned()];
    let view = shell_view(&labels, "https://example.com", CHROME_HEIGHT);
    let layout = layout_shell(800.0, 600.0, 1.0, &view);
    let cols = super::tokens::sidebar_columns(layout.sidebar.width - super::RAIL_CSS - 16.0, 52.0);
    assert!(
        cols <= 3,
        "narrow sidebar must not request too many columns"
    );
    assert!(
        layout.page.x >= layout.sidebar.x + layout.sidebar.width - 1.0,
        "page must start at or after the sidebar edge"
    );
}

#[test]
fn shell_hit_rects_match_painted_controls() {
    let labels = vec!["Alpha".to_owned(), "Beta".to_owned()];
    let mut view = shell_view(&labels, "https://example.com", CHROME_HEIGHT);
    view.can_go_back = true;
    view.can_go_forward = true;
    let layout = layout_shell(1000.0, 700.0, 1.0, &view);
    let content_x = layout.sidebar.x + RAIL_CSS + 8.0;
    let first_row = super::tokens::sidebar_history_top(
        layout.sidebar.y,
        (layout.sidebar.width - RAIL_CSS - 16.0).max(1.0),
    );
    let center = |r: Rect| (r.x + r.width * 0.5, r.y + r.height * 0.5);
    let cases = [
        (ShellHit::ToggleSidebar, layout.toggle),
        (ShellHit::Nav(NavigationButton::Back), layout.nav.back),
        (ShellHit::Nav(NavigationButton::Forward), layout.nav.forward),
        (ShellHit::Nav(NavigationButton::Reload), layout.nav.reload),
        (ShellHit::NewTab, layout.actions[2]),
        (
            ShellHit::Activate(0),
            Rect::new(content_x + 8.0, first_row + 5.0, 80.0, 20.0),
        ),
    ];
    for (hit, rect) in cases {
        let (x, y) = center(rect);
        assert_eq!(
            shell_hit(1000.0, 700.0, 1.0, &view, x, y),
            Some(hit),
            "hit rectangle for {hit:?} must match the painted control"
        );
    }
}

#[test]
fn shell_tab_cycles_focus_over_top_bar_controls() {
    let labels = vec!["Alpha".to_owned(), "Beta".to_owned()];
    let view = shell_view(&labels, "https://example.com", CHROME_HEIGHT);
    let order = shell_focus_order(&view);
    assert_eq!(order.first(), Some(&ShellHit::ToggleSidebar));
    assert_eq!(order[1], ShellHit::Nav(NavigationButton::Back));
    assert_eq!(order[2], ShellHit::Nav(NavigationButton::Forward));
    assert_eq!(order[3], ShellHit::Nav(NavigationButton::Reload));
    assert_eq!(order[4], ShellHit::Activate(0));
    assert_eq!(order[5], ShellHit::Activate(1));
    assert_eq!(order.last(), Some(&ShellHit::NewTab));
    assert_eq!(order.len(), 7);
}

#[test]
fn shell_layout_keeps_sidebar_and_inset_page_distinct() {
    let labels = vec!["Historia de España".to_owned(), "Andalucía".to_owned()];
    let view = shell_view(&labels, "https://es.wikipedia.org", CHROME_HEIGHT);
    let layout = layout_shell(1280.0, 720.0, 1.0, &view);
    assert_eq!(layout.top.height, CHROME_HEIGHT);
    assert!(layout.sidebar.width >= 96.0 && layout.sidebar.width <= SIDEBAR_MAX);
    assert!(layout.rail.width <= layout.sidebar.width);
    assert!(
        layout.page.x > layout.sidebar.x,
        "the page starts beside the sidebar"
    );
    assert!(
        (layout.pill.x + layout.pill.width / 2.0 - 640.0).abs() < 1.0,
        "the address pill is centred like the reference"
    );
    assert_eq!(layout.dots.len(), 3);
    assert_eq!(layout.actions.len(), 4);
}

#[test]
fn shell_top_bar_controls_share_one_row() {
    // UI-TARGET: traffic lights, sidebar toggle, back/forward/reload, the centred
    // URL pill and the right actions all live on one macOS-style row.
    let labels = vec!["Historia de España".to_owned()];
    let view = shell_view(&labels, "https://es.wikipedia.org", CHROME_HEIGHT);
    let layout = layout_shell(1000.0, 700.0, 1.0, &view);
    let center = |r: Rect| r.y + r.height * 0.5;
    let dot = center(layout.dots[0]);
    assert!(
        (center(layout.toggle) - dot).abs() <= 6.0,
        "sidebar toggle must share the traffic-light row"
    );
    assert!(
        (center(layout.nav.back) - dot).abs() <= 6.0,
        "navigation was on a second strip: nav centre={} dot centre={dot}",
        center(layout.nav.back)
    );
    assert!(
        (center(layout.actions[0]) - dot).abs() <= 6.0,
        "right actions must share the row"
    );
    assert!(
        layout.nav.reload.x + layout.nav.reload.width < layout.pill.x,
        "navigation must sit left of the centred URL pill"
    );
}

#[test]
fn shell_paint_uses_measured_dark_surfaces_and_traffic_lights() {
    let labels = vec!["Historia de España".to_owned()];
    let view = shell_view(&labels, "https://es.wikipedia.org", CHROME_HEIGHT);
    let layout = layout_shell(1280.0, 720.0, 1.0, &view);
    let mut pixmap = Pixmap::new(1280, 720);
    paint_shell(&mut pixmap, 1.0, &layout, &view);
    let pixel = |x: u32, y: u32| pixmap.pixels[(y * 1280 + x) as usize];
    let shell = ShellPalette::DARK;
    assert_eq!(pixel(900, 4), shell.top, "top bar is dark charcoal");
    assert_eq!(
        pixel(
            (layout.dots[0].x + 5.0) as u32,
            (layout.dots[0].y + 5.0) as u32
        ),
        shell.traffic[0],
        "red traffic light"
    );
    assert_eq!(
        pixel(
            (layout.sidebar.x + 200.0) as u32,
            (layout.sidebar.y + 130.0) as u32
        ),
        shell.sidebar,
        "sidebar keeps its dark surface"
    );
    assert_ne!(
        pixel((layout.pill.x + 10.0) as u32, (layout.pill.y + 17.0) as u32),
        shell.top,
        "address pill is a raised surface"
    );
}

#[test]
fn shell_hit_matches_sidebar_geometry() {
    let labels = vec!["Alpha".to_owned(), "Beta".to_owned()];
    let view = shell_view(&labels, "https://example.com", CHROME_HEIGHT);
    let layout = layout_shell(640.0, 800.0, 1.0, &view);
    assert_eq!(
        shell_hit(
            640.0,
            800.0,
            1.0,
            &view,
            layout.sidebar.x + 120.0,
            super::tokens::sidebar_history_top(
                layout.sidebar.y,
                (layout.sidebar.width - RAIL_CSS - 16.0).max(1.0),
            ) + 5.0
        ),
        Some(ShellHit::Activate(0)),
        "first history row is clickable"
    );
    assert_eq!(
        shell_hit(640.0, 400.0, 1.0, &view, 400.0, view.top_css + 40.0),
        None
    );
}
