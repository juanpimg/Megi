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
