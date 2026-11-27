//! Back/forward/reload navigation buttons drawn at the left of the address bar.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::drawing::*;
use super::*;

/// Back/forward/reload buttons drawn at the left of the address bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavigationButtons {
    /// Back button.
    pub back: Rect,
    /// Forward button.
    pub forward: Rect,
    /// Reload button.
    pub reload: Rect,
}

/// Hit on the navigation buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationButton {
    /// Go back.
    Back,
    /// Go forward.
    Forward,
    /// Reload.
    Reload,
}

/// Computes the navigation buttons layout (left of the address bar row).
pub fn navigation_buttons(_width: f32) -> NavigationButtons {
    let y = TAB_STRIP_HEIGHT + 4.0;
    let step = NAV_BUTTON_SIZE + NAV_BUTTON_GAP;
    NavigationButtons {
        back: Rect::new(4.0, y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE),
        forward: Rect::new(4.0 + step, y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE),
        reload: Rect::new(4.0 + 2.0 * step, y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE),
    }
}

/// Navigation buttons laid out inside the dark shell's single top bar.
///
/// CSS Grid/Flexbox are not involved: the shell is a fixed, deterministic bar,
/// so the controls share one row centred in `top_css` (docs/UI-TARGET.md).
pub fn navigation_buttons_in_top_bar(_width: f32, top_css: f32, margin: f32) -> NavigationButtons {
    let size = if top_css >= 60.0 { 26.0 } else { 20.0 };
    let y = ((top_css - size) * 0.5).max(2.0);
    let step = size + 4.0;
    let x0 = margin + 96.0;
    NavigationButtons {
        back: Rect::new(x0, y, size, size),
        forward: Rect::new(x0 + step, y, size, size),
        reload: Rect::new(x0 + 2.0 * step, y, size, size),
    }
}

/// Hit-tests the navigation buttons honouring their disabled state.
pub fn navigation_button_at(
    buttons: &NavigationButtons,
    x: f32,
    y: f32,
    can_go_back: bool,
    can_go_forward: bool,
) -> Option<NavigationButton> {
    if can_go_back && contains(buttons.back, x, y) {
        return Some(NavigationButton::Back);
    }
    if can_go_forward && contains(buttons.forward, x, y) {
        return Some(NavigationButton::Forward);
    }
    if contains(buttons.reload, x, y) {
        return Some(NavigationButton::Reload);
    }
    None
}

/// Width reserved by the navigation buttons before the address bar text starts.
pub fn navigation_buttons_width() -> f32 {
    3.0 * NAV_BUTTON_SIZE + 2.0 * NAV_BUTTON_GAP + 6.0
}

pub(crate) fn paint_navigation_buttons(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>) {
    paint_navigation_buttons_at(
        pixmap,
        scale,
        &navigation_buttons(view.css_width),
        view.can_go_back,
        view.can_go_forward,
        None,
        None,
    );
}

/// Paints navigation buttons at explicit rectangles (the shell top bar).
///
/// `hover`/`focus` come from the same [`ShellHit`](super::ShellHit) state used by
/// hit-testing, so the painted affordance and the interactive control agree.
pub(crate) fn paint_navigation_buttons_at(
    pixmap: &mut Pixmap,
    scale: f32,
    buttons: &NavigationButtons,
    can_go_back: bool,
    can_go_forward: bool,
    hover: Option<NavigationButton>,
    focus: Option<NavigationButton>,
) {
    // A subtle rounded surface reads as clickable; one SVG atlas supplies the
    // glyphs so they stay crisp at every device scale.
    let atlas = super::IconAtlas::new();
    let entries = [
        (
            buttons.back,
            super::Icon::Back,
            can_go_back,
            NavigationButton::Back,
        ),
        (
            buttons.forward,
            super::Icon::Forward,
            can_go_forward,
            NavigationButton::Forward,
        ),
        (
            buttons.reload,
            super::Icon::Reload,
            true,
            NavigationButton::Reload,
        ),
    ];
    for (rect, icon, enabled, button) in entries {
        let active = focus == Some(button) || hover == Some(button);
        let surface = if active {
            super::STATE.pressed
        } else {
            COLOR_ADDRESS_BG
        };
        fill_round(pixmap, rect, scale, 4.0, surface);
        let color = if !enabled {
            COLOR_DISABLED
        } else if focus == Some(button) {
            super::STATE.focus
        } else if hover == Some(button) {
            super::STATE.hover
        } else {
            COLOR_ADDRESS_TEXT
        };
        let _ = atlas.paint(pixmap, icon, rect, color, scale);
    }
}
