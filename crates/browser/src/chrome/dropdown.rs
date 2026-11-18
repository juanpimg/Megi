//! Floating `<select>` dropdown: items, geometry, navigation state and painting.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::drawing::*;
use super::*;

/// Compute layout of the floating `<select>` dropdown.
///
/// The list is anchored below the control (or above it when there is no room below), capped
/// to [`DROPDOWN_MAX_VISIBLE`] rows; `scroll` selects the first visible option.
pub fn dropdown_layout(
    control: &crate::links::CssRect,
    option_count: usize,
    css_viewport_height: u32,
    scroll_y: u32,
    scroll: usize,
) -> DropdownList {
    let items: Vec<DropdownItem> = (0..option_count)
        .map(|index| DropdownItem {
            label: index.to_string(),
            option_index: Some(index),
        })
        .collect();
    dropdown_layout_items(control, &items, css_viewport_height, scroll_y, scroll)
}

/// [`dropdown_layout`] for real dropdown rows (group headers included).
pub fn dropdown_layout_items(
    control: &crate::links::CssRect,
    items: &[DropdownItem],
    css_viewport_height: u32,
    scroll_y: u32,
    scroll: usize,
) -> DropdownList {
    let option_count = items.len();
    let rows = option_count.min(DROPDOWN_MAX_VISIBLE);
    let height = rows as f32 * DROPDOWN_ROW_HEIGHT + 4.0;
    let viewport = css_viewport_height as f32;
    let control_top = control.y - scroll_y as f32;
    let control_bottom = control_top + control.height;
    let (top, above) = if control_bottom + height <= viewport {
        (control_bottom + 1.0, false)
    } else {
        ((control_top - height - 1.0).max(0.0), true)
    };
    let width = control.width.max(80.0);
    let rect = Rect::new(control.x, top, width, height);
    let scroll = scroll.min(option_count.saturating_sub(rows));
    let mut row_rects = Vec::with_capacity(rows);
    for index in 0..rows {
        row_rects.push(Rect::new(
            rect.x + 1.0,
            rect.y + 2.0 + index as f32 * DROPDOWN_ROW_HEIGHT,
            rect.width - 2.0,
            DROPDOWN_ROW_HEIGHT,
        ));
    }
    DropdownList {
        rect,
        rows: row_rects,
        visible: rows,
        first: scroll,
        total: option_count,
        above,
        items: items.to_vec(),
    }
}

/// One row of the `<select>` dropdown: either an `<optgroup>` header (not selectable) or an
/// option (mapped to its index in the control's options).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropdownItem {
    /// Row label.
    pub label: String,
    /// Option index, or `None` for a non-selectable group header.
    pub option_index: Option<usize>,
}

impl DropdownItem {
    /// Whether this row is a selectable option.
    pub const fn is_option(&self) -> bool {
        self.option_index.is_some()
    }
}

/// Builds dropdown rows from select options, inserting one non-selectable header per
/// `<optgroup label>` change.
pub fn dropdown_items(options: &[crate::forms::SelectOption]) -> Vec<DropdownItem> {
    let mut items = Vec::with_capacity(options.len());
    let mut current_group: Option<&str> = None;
    for (index, option) in options.iter().enumerate() {
        let group = option.group.as_deref();
        if let Some(group) = group
            && current_group != Some(group)
        {
            items.push(DropdownItem {
                label: group.to_string(),
                option_index: None,
            });
        }
        if group.is_none() {
            current_group = None;
        } else {
            current_group = group;
        }
        items.push(DropdownItem {
            label: option.label.clone(),
            option_index: Some(index),
        });
    }
    items
}

/// Computed dropdown geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct DropdownList {
    /// Floating list rectangle.
    pub rect: Rect,
    /// Visible row rectangles.
    pub rows: Vec<Rect>,
    /// Number of visible rows.
    pub visible: usize,
    /// Index of the first visible option.
    pub first: usize,
    /// Total number of dropdown items (options + group headers).
    pub total: usize,
    /// Whether the list opens above the control.
    pub above: bool,
    /// Dropdown items (group headers are not selectable).
    pub items: Vec<DropdownItem>,
}

impl DropdownList {
    /// Option index under a chrome point (`None` on a group header or outside the list).
    pub fn hit_option(&self, x: f32, y: f32) -> Option<usize> {
        for (row, rect) in self.rows.iter().enumerate() {
            if contains(*rect, x, y) {
                return self
                    .items
                    .get(self.first + row)
                    .and_then(|item| item.option_index);
            }
        }
        None
    }

    /// Row index under a chrome point (group headers included).
    pub fn hit_row(&self, x: f32, y: f32) -> Option<usize> {
        for (row, rect) in self.rows.iter().enumerate() {
            if contains(*rect, x, y) {
                return Some(self.first + row);
            }
        }
        None
    }

    /// Whether the point is inside the list (including borders).
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        contains(self.rect, x, y)
    }
}

/// Open `<select>` dropdown state.
#[derive(Clone, Debug, PartialEq)]
pub struct DropdownState {
    /// Stable form id of the `<select>`.
    pub form_id: u32,
    /// Control rectangle in page CSS coordinates.
    pub control_rect: crate::links::CssRect,
    /// Select options.
    pub options: Vec<crate::forms::SelectOption>,
    /// First visible option.
    pub scroll: usize,
    /// Hovered/active option (highlighted).
    pub active: Option<usize>,
}

impl DropdownState {
    /// Opens a dropdown for `control`.
    pub fn open(
        form_id: u32,
        control_rect: crate::links::CssRect,
        options: Vec<crate::forms::SelectOption>,
    ) -> Self {
        let active = options.iter().position(|option| option.selected);
        Self {
            form_id,
            control_rect,
            options,
            scroll: 0,
            active,
        }
    }

    /// Closes the dropdown (state is dropped by the caller).
    pub fn close(&mut self) {
        self.options.clear();
    }

    /// Scrolls the visible window by `direction` rows (negative = up), clamped.
    pub fn scroll_by(&mut self, direction: i32) -> bool {
        let max = self.options.len().saturating_sub(DROPDOWN_MAX_VISIBLE);
        let next = (self.scroll as i32 + direction).clamp(0, max as i32) as usize;
        let changed = next != self.scroll;
        self.scroll = next;
        changed
    }

    /// Active option index.
    pub const fn active(&self) -> Option<usize> {
        self.active
    }

    /// Dropdown rows (group headers included).
    pub fn items(&self) -> Vec<DropdownItem> {
        dropdown_items(&self.options)
    }

    /// Number of dropdown rows.
    pub fn row_count(&self) -> usize {
        self.items().len()
    }

    /// Moves the active row to the next/previous **selectable** option, skipping group
    /// headers and scrolling the window when needed.
    pub fn move_active(&mut self, down: bool) -> bool {
        let items = self.items();
        if !items.iter().any(DropdownItem::is_option) {
            return false;
        }
        let mut row = self.active.unwrap_or(0);
        let step = if down { 1 } else { items.len() - 1 };
        for _ in 0..items.len() {
            row = (row + step) % items.len();
            if items.get(row).is_some_and(DropdownItem::is_option) {
                break;
            }
        }
        self.active = Some(row);
        self.scroll_to_active(items.len());
        true
    }

    /// Home / End: first or last selectable option.
    pub fn move_home(&mut self) -> bool {
        let items = self.items();
        let Some(row) = items.iter().position(DropdownItem::is_option) else {
            return false;
        };
        self.active = Some(row);
        self.scroll = 0;
        true
    }

    /// End: last selectable option.
    pub fn move_end(&mut self) -> bool {
        let items = self.items();
        let Some(row) = items.iter().rposition(DropdownItem::is_option) else {
            return false;
        };
        self.active = Some(row);
        self.scroll_to_active(items.len());
        true
    }

    /// PageUp / PageDown: jump one page (visible rows) between selectable options.
    pub fn move_page(&mut self, down: bool) -> bool {
        let items = self.items();
        let selectable: Vec<usize> = items
            .iter()
            .enumerate()
            .filter_map(|(row, item)| item.is_option().then_some(row))
            .collect();
        if selectable.is_empty() {
            return false;
        }
        let current = self.active.unwrap_or(selectable[0]);
        let target = if down {
            let position = selectable
                .iter()
                .position(|row| *row >= current)
                .unwrap_or(0);
            selectable[(position + DROPDOWN_MAX_VISIBLE).min(selectable.len() - 1)]
        } else {
            let position = selectable
                .iter()
                .rposition(|row| *row <= current)
                .unwrap_or(0);
            selectable[position.saturating_sub(DROPDOWN_MAX_VISIBLE)]
        };
        self.active = Some(target);
        self.scroll_to_active(items.len());
        true
    }

    /// First-character typeahead over option labels (case-insensitive, skips headers).
    pub fn first_char(&mut self, character: char) -> bool {
        let items = self.items();
        let character = character.to_ascii_lowercase();
        let Some(row) = items.iter().position(|item| {
            item.is_option() && item.label.to_ascii_lowercase().starts_with(character)
        }) else {
            return false;
        };
        self.active = Some(row);
        self.scroll_to_active(items.len());
        true
    }

    fn scroll_to_active(&mut self, total: usize) {
        if let Some(active) = self.active {
            if active < self.scroll {
                self.scroll = active;
            } else if active >= self.scroll + DROPDOWN_MAX_VISIBLE {
                self.scroll = active + 1 - DROPDOWN_MAX_VISIBLE;
            }
            self.scroll = self.scroll.min(total.saturating_sub(DROPDOWN_MAX_VISIBLE));
        }
    }

    /// Whether the dropdown is non-empty.
    pub fn is_open(&self) -> bool {
        !self.options.is_empty()
    }
}

pub(crate) fn paint_dropdown(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    dropdown: &DropdownState,
) {
    let items = dropdown.items();
    let list = dropdown_layout_items(
        &dropdown.control_rect,
        &items,
        view.css_viewport_height,
        view.scroll_y,
        dropdown.scroll,
    );
    paint_panel(pixmap, list.rect, scale, MENU_RADIUS, COLOR_DROPDOWN_BG);
    for (row, rect) in list.rows.iter().enumerate() {
        let item_index = list.first + row;
        let Some(item) = items.get(item_index) else {
            break;
        };
        let option = item
            .option_index
            .and_then(|index| dropdown.options.get(index));
        let background = if item.option_index.is_none() {
            COLOR_GROUP_HEADER
        } else if dropdown.active == Some(item_index) {
            COLOR_DROPDOWN_ACTIVE
        } else if option.is_some_and(|option| option.selected) {
            COLOR_DROPDOWN_SELECTED
        } else {
            COLOR_DROPDOWN_BG
        };
        pixmap.fill_rect(scaled(*rect, scale), background);
        if item.option_index.is_none() {
            draw_text(
                pixmap,
                &format!("{}:", item.label),
                rect.x + 5.0,
                rect.y + 5.0,
                scale,
                11.0,
                COLOR_SCROLL,
            );
        } else {
            draw_text(
                pixmap,
                &item.label,
                rect.x + 9.0,
                rect.y + 5.0,
                scale,
                11.0,
                COLOR_ADDRESS_TEXT,
            );
        }
    }
    // Minimal scrollbar when the list exceeds the visible cap.
    if items.len() > list.visible {
        let track = Rect::new(
            list.rect.x + list.rect.width - 6.0,
            list.rect.y + 2.0,
            3.0,
            list.rect.height - 4.0,
        );
        pixmap.fill_rect(scaled(track, scale), COLOR_SCROLL_TRACK);
        let ratio = list.visible as f32 / items.len() as f32;
        let thumb_height = (track.height * ratio).max(8.0);
        let max_scroll = items.len().saturating_sub(list.visible).max(1) as f32;
        let offset = (list.first as f32 / max_scroll) * (track.height - thumb_height).max(0.0);
        pixmap.fill_rect(
            scaled(
                Rect::new(track.x, track.y + offset, track.width, thumb_height),
                scale,
            ),
            COLOR_SCROLL,
        );
    }
}
