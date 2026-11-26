//! Context menu: actions, state, geometry, hit-testing and painting.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::drawing::*;
use super::*;

/// Action of a context-menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Open the target link in a new tab.
    OpenLinkInNewTab,
    /// Copy the target link to the internal clipboard.
    CopyLinkAddress,
    /// Save the target image resource into the download directory.
    SaveImage,
    /// Copy the image (documented no-op: no image clipboard).
    CopyImage,
    /// Reload the page.
    Reload,
    /// Save the page as PNG (same path as Ctrl+S).
    SavePagePng,
    /// Save the page as serialized HTML (same path as Ctrl+Shift+S).
    SavePageHtml,
    /// Download CSS/JS/font/image subresources as a bounded batch.
    SavePageResources,
}

impl MenuAction {
    /// Whether executing this action has an effect (copy-image is a documented no-op).
    pub const fn is_implemented(self) -> bool {
        !matches!(self, Self::CopyImage)
    }
}

/// One context-menu row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMenuItem {
    /// Action executed when the row is chosen.
    pub action: MenuAction,
    /// Visible label.
    pub label: &'static str,
}

/// Context menu state: target under the cursor, rows, selected row and anchor position.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenu {
    /// Link URL under the cursor, if any.
    pub target_link: Option<String>,
    /// Image under the cursor, if any.
    pub target_image: Option<crate::images::ImageRegion>,
    /// Menu rows (contextual first, page actions last).
    pub items: Vec<ContextMenuItem>,
    /// Selected row index (keyboard/hover).
    pub selected: usize,
    /// Anchor position (chrome CSS coordinates).
    pub x: f32,
    /// Anchor position (chrome CSS coordinates).
    pub y: f32,
}

impl ContextMenu {
    /// Builds the contextual menu for the given targets.
    pub fn new(
        target_link: Option<String>,
        target_image: Option<crate::images::ImageRegion>,
        x: f32,
        y: f32,
    ) -> Self {
        let strings = crate::i18n::strings(crate::i18n::Locale::current());
        let mut items = Vec::new();
        if target_link.is_some() {
            items.push(ContextMenuItem {
                action: MenuAction::OpenLinkInNewTab,
                label: strings.open_link_in_new_tab,
            });
            items.push(ContextMenuItem {
                action: MenuAction::CopyLinkAddress,
                label: strings.copy_link_address,
            });
        }
        if target_image.is_some() {
            items.push(ContextMenuItem {
                action: MenuAction::SaveImage,
                label: strings.save_image,
            });
            items.push(ContextMenuItem {
                action: MenuAction::CopyImage,
                label: strings.copy_image_unsupported,
            });
        }
        items.push(ContextMenuItem {
            action: MenuAction::Reload,
            label: strings.reload,
        });
        items.push(ContextMenuItem {
            action: MenuAction::SavePagePng,
            label: strings.save_page_as_png,
        });
        items.push(ContextMenuItem {
            action: MenuAction::SavePageHtml,
            label: strings.save_page_as_html,
        });
        items.push(ContextMenuItem {
            action: MenuAction::SavePageResources,
            label: strings.save_page_resources,
        });
        Self {
            target_link,
            target_image,
            items,
            selected: 0,
            x,
            y,
        }
    }

    /// Moves the selection up/down with wrap-around.
    pub fn move_selection(&mut self, down: bool) -> bool {
        if self.items.is_empty() {
            return false;
        }
        self.selected = if down {
            (self.selected + 1) % self.items.len()
        } else if self.selected == 0 {
            self.items.len() - 1
        } else {
            self.selected - 1
        };
        true
    }

    /// Currently selected action.
    pub fn selected_action(&self) -> Option<MenuAction> {
        self.items.get(self.selected).map(|item| item.action)
    }
}

/// Computed context-menu geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextMenuLayout {
    /// Menu rectangle.
    pub rect: Rect,
    /// Row rectangles (one per item).
    pub rows: Vec<Rect>,
}

/// Computes the context-menu layout anchored at the cursor and clamped to the window.
pub fn context_menu_layout(
    menu: &ContextMenu,
    css_width: f32,
    css_height: u32,
) -> ContextMenuLayout {
    let width = CONTEXT_MENU_WIDTH.min((css_width - 8.0).max(80.0));
    let height = menu.items.len() as f32 * CONTEXT_MENU_ROW_HEIGHT + 4.0;
    let x = menu.x.min((css_width - width - 2.0).max(2.0)).max(2.0);
    let below = menu.y + height + 2.0 <= css_height as f32;
    let y = if below {
        menu.y + 2.0
    } else {
        (menu.y - height - 2.0).max(2.0)
    };
    let rect = Rect::new(x, y, width, height);
    let rows = (0..menu.items.len())
        .map(|index| {
            Rect::new(
                rect.x + 1.0,
                rect.y + 2.0 + index as f32 * CONTEXT_MENU_ROW_HEIGHT,
                rect.width - 2.0,
                CONTEXT_MENU_ROW_HEIGHT,
            )
        })
        .collect();
    ContextMenuLayout { rect, rows }
}

/// Row index under a chrome point.
pub fn context_menu_hit(layout: &ContextMenuLayout, x: f32, y: f32) -> Option<usize> {
    layout.rows.iter().position(|rect| contains(*rect, x, y))
}

pub(crate) fn paint_context_menu(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    menu: &ContextMenu,
) {
    let layout = context_menu_layout(menu, view.css_width, view.css_viewport_height);
    paint_panel(pixmap, layout.rect, scale, MENU_RADIUS, COLOR_MENU_BG);
    for (index, row) in layout.rows.iter().enumerate() {
        let Some(item) = menu.items.get(index) else {
            break;
        };
        if index == menu.selected {
            paint_menu_selection(pixmap, *row, scale);
        }
        draw_text(
            pixmap,
            item.label,
            row.x + 10.0,
            row.y + 4.0,
            scale,
            MENU_TEXT_SIZE,
            COLOR_ADDRESS_TEXT,
        );
    }
}

use super::drawing::{draw_text, fill_round};

/// Draws the Save-as panel over the framebuffer.
pub fn draw_save_as_dialog(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    name: &str,
    format: &str,
) {
    if dest_width == 0 || dest_height == 0 {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = dest_width as f32 / scale;
    let panel_width = 420.0_f32.min(css_width - 24.0).max(160.0);
    let panel_height = 108.0_f32.min(dest_height as f32 / scale - 24.0).max(60.0);
    let x = (css_width - panel_width) / 2.0;
    let y = 110.0_f32
        .min(dest_height as f32 / scale - panel_height - 8.0)
        .max(8.0);
    fill_round(
        &mut pixmap,
        Rect::new(x, y, panel_width, panel_height),
        scale,
        8.0,
        Color::new(0x20, 0x24, 0x2E, 0xFF),
    );
    draw_text(
        &mut pixmap,
        "Save as:",
        x + 14.0,
        y + 24.0,
        scale,
        15.0,
        Color::WHITE,
    );
    draw_text(
        &mut pixmap,
        &format!("{name}  [{format}]  (Tab format, Enter save, Esc cancel)"),
        x + 14.0,
        y + 58.0,
        scale,
        14.0,
        Color::WHITE,
    );
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 || index >= dest.len() {
            continue;
        }
        if color.a == 255 {
            dest[index] = u32::from(color.r) << 24
                | u32::from(color.g) << 16
                | u32::from(color.b) << 8
                | 0xFF;
        }
    }
}

/// Draws the downloads panel (F7-SAVE-AS): one line per job with the selected one highlighted.
pub fn draw_downloads_panel(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    lines: &[String],
    selected: usize,
) {
    if dest_width == 0 || dest_height == 0 || lines.is_empty() {
        return;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = dest_width as f32 / scale;
    let panel_width = 460.0_f32.min(css_width - 24.0).max(180.0);
    let row_height = 24.0_f32;
    let panel_height = (lines.len() as f32 * row_height + 42.0)
        .min(dest_height as f32 / scale - 24.0)
        .max(60.0);
    let x = (css_width - panel_width) / 2.0;
    let y = 80.0_f32
        .min(dest_height as f32 / scale - panel_height - 8.0)
        .max(8.0);
    drawing::fill_round(
        &mut pixmap,
        Rect::new(x, y, panel_width, panel_height),
        scale,
        8.0,
        Color::new(0x20, 0x24, 0x2E, 0xFF),
    );
    drawing::draw_text(
        &mut pixmap,
        "Downloads  (↑↓ select, P pause/resume, C cancel, Esc close)",
        x + 12.0,
        y + 20.0,
        scale,
        13.0,
        Color::WHITE,
    );
    for (index, line) in lines.iter().enumerate() {
        let row_y = y + 40.0 + index as f32 * row_height;
        if index == selected {
            pixmap.fill_rect(
                Rect::new(
                    x * scale + 6.0,
                    row_y * scale,
                    panel_width * scale - 12.0,
                    row_height * scale,
                ),
                Color::new(0x35, 0x4A, 0x6A, 0xFF),
            );
        }
        drawing::draw_text(
            &mut pixmap,
            line,
            x + 14.0,
            row_y + 16.0,
            scale,
            12.0,
            Color::WHITE,
        );
    }
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 || index >= dest.len() {
            continue;
        }
        if color.a == 255 {
            dest[index] = u32::from(color.r) << 24
                | u32::from(color.g) << 16
                | u32::from(color.b) << 8
                | 0xFF;
        }
    }
}

use super::drawing::*;

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
            layout.rect.x + 10.0,
            layout.rect.y + 6.0,
            scale,
            MENU_TEXT_SIZE,
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
            ((row.width - 34.0) / 8.0).max(8.0) as usize,
        );
        draw_text(
            pixmap,
            &label,
            row.x + 26.0,
            row.y + 4.0,
            scale,
            MENU_TEXT_SIZE,
            COLOR_ADDRESS_TEXT,
        );
    }
}
