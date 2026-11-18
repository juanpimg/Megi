//! Ctrl+H history menu: geometry, hit-testing and painting.

use megi_webview::layout::Rect;
use megi_webview::paint::{Color, Pixmap};

use super::drawing::*;
use super::*;

/// History-menu geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryMenuLayout {
    /// Menu rectangle.
    pub rect: Rect,
    /// Rows (one per entry).
    pub rows: Vec<Rect>,
}

/// Computes the Ctrl+H history menu layout below the address bar.
pub fn history_menu_layout(
    menu: &crate::history::HistoryMenu,
    css_width: f32,
    css_height: u32,
) -> HistoryMenuLayout {
    let width = 320.0_f32.min((css_width - 8.0).max(120.0));
    let height = menu.entries.len().max(1) as f32 * 20.0 + 4.0;
    let x = menu.x.min((css_width - width - 2.0).max(2.0)).max(2.0);
    let y = (menu.y + 2.0).min((css_height as f32 - height - 2.0).max(TAB_STRIP_HEIGHT));
    let rect = Rect::new(x, y, width, height);
    let rows = (0..menu.entries.len())
        .map(|index| {
            Rect::new(
                rect.x + 1.0,
                rect.y + 2.0 + index as f32 * 20.0,
                rect.width - 2.0,
                20.0,
            )
        })
        .collect();
    HistoryMenuLayout { rect, rows }
}

/// Row index under a chrome point.
pub fn history_menu_hit(layout: &HistoryMenuLayout, x: f32, y: f32) -> Option<usize> {
    layout.rows.iter().position(|rect| contains(*rect, x, y))
}

pub(crate) fn paint_history_menu(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    menu: &crate::history::HistoryMenu,
) {
    let layout = history_menu_layout(menu, view.css_width, view.css_viewport_height);
    paint_panel(pixmap, layout.rect, scale, MENU_RADIUS, COLOR_MENU_BG);
    if menu.entries.is_empty() {
        let strings = crate::i18n::strings(crate::i18n::Locale::current());
        draw_text(
            pixmap,
            strings.no_history_menu,
            layout.rect.x + 8.0,
            layout.rect.y + 6.0,
            scale,
            11.0,
            COLOR_SCROLL,
        );
        return;
    }
    for (index, row) in layout.rows.iter().enumerate() {
        let Some(entry) = menu.entries.get(index) else {
            break;
        };
        if index == menu.selected {
            paint_menu_selection(pixmap, *row, scale);
        }
        let (letter, color) = crate::history::domain_badge(&entry.url);
        let badge = Rect::new(row.x + 4.0, row.y + 3.0, 14.0, 14.0);
        pixmap.fill_rect(
            scaled(badge, scale),
            Color::new(color[0], color[1], color[2], 0xFF),
        );
        draw_text(
            pixmap,
            &letter.to_string(),
            badge.x + 4.0,
            badge.y + 2.0,
            scale,
            10.0,
            Color::WHITE,
        );
        let label = truncate_title(
            &format!("{} — {}", entry.title, entry.url),
            ((row.width - 30.0) / 6.6).max(8.0) as usize,
        );
        draw_text(
            pixmap,
            &label,
            row.x + 24.0,
            row.y + 5.0,
            scale,
            11.0,
            COLOR_ADDRESS_TEXT,
        );
    }
}
