//! Pure vertical tab rail: layout, hit-testing and painting.
//!
//! The rail is a chrome overlay anchored below the top band. Its geometry is a
//! pure function of the viewport and [`RailView`]; no shell state, network or
//! page layout is involved. Painting stays inside the rail columns so the page
//! rows and the horizontal chrome keep their existing pixels (Phase 1 visual
//! slice of the Zen-inspired UI plan).

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::drawing::{contains, draw_text, fill_round, paint_favicon};
use super::*;

/// Minimum height of one rail row, in physical pixels.
const ROW_MIN_PHYSICAL: f32 = 22.0;
/// Width of the close affordance in an expanded row, in CSS pixels.
const CLOSE_WIDTH: f32 = 14.0;
/// Height of the close affordance, in CSS pixels.
const CLOSE_HEIGHT: f32 = 12.0;
/// Conservative CSS width of one 11 px glyph, used to truncate rail titles.
const GLYPH_WIDTH: f32 = 8.0;
/// Text size for rail titles and glyphs, in CSS pixels.
const TEXT_SIZE: f32 = 11.0;

/// Rectangle, close target and accessible name of one rail tab.
#[derive(Clone, Debug, PartialEq)]
pub struct RailTabRect {
    /// Tab index, shared with the horizontal tab strip.
    pub index: usize,
    /// Row rectangle in chrome CSS coordinates.
    pub rect: Rect,
    /// Close target, or [`Rect::ZERO`] when titles are hidden.
    pub close: Rect,
    /// Truncated title when expanded, empty when collapsed.
    pub title: String,
}

/// Computed geometry of the vertical rail.
#[derive(Clone, Debug, PartialEq)]
pub struct RailLayout {
    /// Rail surface: `(0, CHROME_HEIGHT, rail_width, height - CHROME_HEIGHT)`.
    pub rect: Rect,
    /// One row per visible tab, top to bottom.
    pub tabs: Vec<RailTabRect>,
    /// New-tab button below the tab rows.
    pub new_tab: Rect,
    /// Expand/collapse toggle, always the first row.
    pub toggle: Rect,
}

/// A hit on the vertical rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailHit {
    /// Activate the tab at this index.
    Activate(usize),
    /// Close the tab at this index.
    Close(usize),
    /// Open a new tab.
    NewTab,
    /// Toggle rail expansion.
    Toggle,
}

/// Pure input data for rail layout and painting.
#[derive(Clone, Copy, Debug)]
pub struct RailView<'a> {
    /// Tab titles, in the same order as the horizontal strip.
    pub labels: &'a [String],
    /// Active tab index.
    pub active: usize,
    /// Hovered tab index, if any.
    pub hover: Option<usize>,
    /// Whether titles and close targets are visible.
    pub expanded: bool,
    /// Rail width in CSS pixels (collapsed, expanded or peek token).
    pub width_css: f32,
}

fn safe_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Computes rail geometry. The toggle is the first row, tab rows follow, and the
/// new-tab button is last. Rows are at least 22 physical pixels tall and never
/// extend past the rail rectangle.
#[must_use]
pub fn layout_rail(width: f32, height: f32, scale: f32, view: &RailView<'_>) -> RailLayout {
    let scale = safe_scale(scale);
    let row_h = (ROW_MIN_PHYSICAL / scale).max(ROW_MIN_PHYSICAL);
    let rail_width = view
        .width_css
        .max(0.0)
        .min(width.max(0.0))
        .min(RAIL_PEEK_WIDTH);
    let rail_height = (height - CHROME_HEIGHT).max(0.0);
    let rect = Rect::new(0.0, CHROME_HEIGHT, rail_width, rail_height);

    // Reserve the toggle and new-tab rows; only whole rows fit in the rail.
    let rows = (rail_height / row_h).floor() as usize;
    let tab_count = rows.saturating_sub(2).min(view.labels.len());
    let row_y = |row: usize| CHROME_HEIGHT + row as f32 * row_h;
    let toggle = if rows > 0 {
        Rect::new(0.0, CHROME_HEIGHT, rail_width, row_h)
    } else {
        Rect::new(0.0, CHROME_HEIGHT, rail_width, 0.0)
    };

    let max_chars = ((rail_width - CLOSE_WIDTH - 26.0) / GLYPH_WIDTH).max(1.0) as usize;
    let mut tabs = Vec::with_capacity(tab_count);
    for index in 0..tab_count {
        let tab_rect = Rect::new(0.0, row_y(index + 1), rail_width, row_h);
        let close = if view.expanded {
            let close_width = CLOSE_WIDTH.min(rail_width);
            Rect::new(
                (rail_width - close_width - 2.0).max(0.0),
                tab_rect.y + (row_h - CLOSE_HEIGHT).max(0.0) / 2.0,
                close_width,
                CLOSE_HEIGHT.min(row_h),
            )
        } else {
            Rect::ZERO
        };
        let label = view.labels.get(index).map_or("", String::as_str);
        let title = if view.expanded {
            truncate_title(label, max_chars)
        } else {
            String::new()
        };
        tabs.push(RailTabRect {
            index,
            rect: tab_rect,
            close,
            title,
        });
    }
    let new_tab = if rows >= 2 {
        Rect::new(0.0, row_y(tab_count + 1), rail_width, row_h)
    } else {
        Rect::ZERO
    };
    RailLayout {
        rect,
        tabs,
        new_tab,
        toggle,
    }
}

/// Hit-tests the rail. Close targets are checked before tab bodies because they
/// sit inside them; the top band is never a rail hit.
#[must_use]
pub fn rail_hit(layout: &RailLayout, x: f32, y: f32) -> Option<RailHit> {
    if y < CHROME_HEIGHT || !contains(layout.rect, x, y) {
        return None;
    }
    if contains(layout.toggle, x, y) {
        return Some(RailHit::Toggle);
    }
    for tab in &layout.tabs {
        if contains(tab.close, x, y) {
            return Some(RailHit::Close(tab.index));
        }
        if contains(tab.rect, x, y) {
            return Some(RailHit::Activate(tab.index));
        }
    }
    if contains(layout.new_tab, x, y) {
        return Some(RailHit::NewTab);
    }
    None
}

/// Paints the rail into `dest`. Deterministic: no time, randomness or network.
pub fn paint_rail(dest: &mut Pixmap, scale: f32, layout: &RailLayout, view: &RailView<'_>) {
    if layout.rect.width <= 0.0 || layout.rect.height <= 0.0 {
        return;
    }
    let scale = safe_scale(scale);
    fill_round(dest, layout.rect, scale, 0.0, COLOR_TAB_STRIP);

    let toggle = layout.toggle;
    fill_round(dest, toggle, scale, 3.0, COLOR_TAB_ACTIVE);
    dest.fill_rect(
        Rect::new(
            toggle.x * scale,
            toggle.y * scale,
            2.0 * scale,
            toggle.height * scale,
        ),
        ACCENT,
    );
    draw_text(
        dest,
        if view.expanded { "<" } else { ">" },
        toggle.x + 5.0,
        toggle.y + 5.0,
        scale,
        TEXT_SIZE,
        COLOR_TEXT,
    );

    for tab in &layout.tabs {
        let active = tab.index == view.active;
        let hovered = view.hover == Some(tab.index);
        let surface = if active {
            COLOR_TAB_ACTIVE
        } else if hovered {
            COLOR_DROPDOWN_ACTIVE
        } else {
            COLOR_TAB
        };
        fill_round(dest, tab.rect, scale, 3.0, surface);
        if active {
            dest.fill_rect(
                Rect::new(
                    tab.rect.x * scale,
                    tab.rect.y * scale,
                    2.0 * scale,
                    tab.rect.height * scale,
                ),
                ACCENT,
            );
        }
        let label = view.labels.get(tab.index).map_or("", String::as_str);
        let icon = crate::favicon::Favicon::fallback(label);
        if view.expanded {
            paint_favicon(dest, &icon, tab.rect.x + 3.0, tab.rect.y + 3.0, scale);
            draw_text(
                dest,
                &tab.title,
                tab.rect.x + 22.0,
                tab.rect.y + 6.0,
                scale,
                TEXT_SIZE,
                COLOR_TEXT,
            );
            let close = tab.close;
            if close.width > 0.0 && close.height > 0.0 {
                fill_round(dest, close, scale, 3.0, COLOR_ADDRESS_BG);
                draw_text(
                    dest,
                    "x",
                    close.x + 3.0,
                    close.y + 2.0,
                    scale,
                    TEXT_SIZE,
                    COLOR_TEXT,
                );
            }
        } else {
            paint_favicon(dest, &icon, tab.rect.x + 2.0, tab.rect.y + 7.0, scale);
        }
    }

    let new_tab = layout.new_tab;
    if new_tab.width > 0.0 && new_tab.height > 0.0 {
        draw_text(
            dest,
            "+",
            new_tab.x + 5.0,
            new_tab.y + 5.0,
            scale,
            14.0,
            COLOR_TEXT,
        );
    }
}

/// Paints the rail over an RGBA page pixmap using the same alpha blend as
/// [`draw_chrome_over_pixmap`](super::draw_chrome_over_pixmap). Returns the
/// number of touched pixels, or zero when the view has no rail.
pub fn draw_rail_over_pixmap(dest: &mut Pixmap, scale: f32, view: &ChromeView<'_>) -> usize {
    let Some(rail) = view.rail else {
        return 0;
    };
    let width = dest.width;
    let height = dest.height;
    if width == 0 || height == 0 {
        return 0;
    }
    let scale = safe_scale(scale);
    let layout = layout_rail(width as f32 / scale, height as f32 / scale, scale, &rail);
    let mut layer = Pixmap::new(width, height);
    paint_rail(&mut layer, scale, &layout, &rail);

    let mut painted = 0usize;
    for (index, color) in layer.pixels.iter().enumerate() {
        if color.a == 0 {
            continue;
        }
        if color.a == 255 {
            dest.pixels[index] = Color::new(color.r, color.g, color.b, 0xFF);
            painted += 1;
            continue;
        }
        let base = dest.pixels[index];
        let alpha = f32::from(color.a) / 255.0;
        let mix = |channel: u8, target: u8| -> u8 {
            let value = f32::from(channel) * (1.0 - alpha) + f32::from(target) * alpha;
            value.round().clamp(0.0, 255.0) as u8
        };
        dest.pixels[index] = Color::new(
            mix(base.r, color.r),
            mix(base.g, color.g),
            mix(base.b, color.b),
            0xFF,
        );
        painted += 1;
    }
    painted
}
