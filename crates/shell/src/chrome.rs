//! Browser chrome drawn by the shell over the rendered page: tab strip, address bar and the
//! floating `<select>` dropdown.
//!
//! The chrome is an **overlay** painted on the physical framebuffer after the page frame
//! (documented trade-off: the rendered page keeps its full viewport, so the top
//! [`CHROME_HEIGHT`] CSS pixels are occluded by the chrome; clicks there are handled by the
//! chrome and never reach the page). All geometry is expressed in CSS pixels and scaled by
//! the effective device scale when painted. The drawing code is pure (a `0RGB` buffer in,
//! pixels out) so it is unit-tested without a display; headless screenshots can include the
//! chrome with `--chrome`.
//!
//! Deliberate limits: no autocomplete, no visual history dropdown, no favicon network
//! fetch in the chrome painter (the worker supplies decoded 16x16 icons or a dot),
//! no native menus, ASCII text rendered with the paint crate's built-in 8×8 bitmap font
//! (uppercase glyphs used for lowercase input), and the address bar edits the URL only
//! (search terms are treated as URLs). The tab strip shows a compact title per tab with
//! `+`/`×` affordances and a `+N` overflow marker; there is no drag/reorder.

use layout::Rect;
use paint::{Color, Pixmap};

use crate::forms::SelectOption;
use crate::links::CssRect;

/// Height of the tab strip, in CSS pixels.
pub const TAB_STRIP_HEIGHT: f32 = 30.0;
/// Height of the address bar, in CSS pixels.
pub const ADDRESS_BAR_HEIGHT: f32 = 28.0;
/// Total chrome height, in CSS pixels (tab strip + address bar).
pub const CHROME_HEIGHT: f32 = TAB_STRIP_HEIGHT + ADDRESS_BAR_HEIGHT;
/// Height of one dropdown option row, in CSS pixels.
pub const DROPDOWN_ROW_HEIGHT: f32 = 18.0;
/// Maximum number of visible dropdown rows before the list scrolls.
pub const DROPDOWN_MAX_VISIBLE: usize = 8;
/// Minimum width of a tab, in CSS pixels.
pub const TAB_MIN_WIDTH: f32 = 72.0;
/// Maximum width of a tab, in CSS pixels.
pub const TAB_MAX_WIDTH: f32 = 200.0;
/// Width reserved for the `+` new-tab button, in CSS pixels.
pub const NEW_TAB_WIDTH: f32 = 28.0;
/// Width of the tab close affordance, in CSS pixels.
pub const TAB_CLOSE_WIDTH: f32 = 14.0;
/// Width reserved for the `+N` overflow marker, in CSS pixels.
pub const TAB_OVERFLOW_WIDTH: f32 = 34.0;

/// Tab strip background.
pub const COLOR_TAB_STRIP: Color = Color::new(0x20, 0x21, 0x24, 0xFF);
/// Inactive tab background.
pub const COLOR_TAB: Color = Color::new(0x35, 0x37, 0x3B, 0xFF);
/// Active tab background.
pub const COLOR_TAB_ACTIVE: Color = Color::new(0x4A, 0x4D, 0x52, 0xFF);
/// Chrome text (tabs/address).
pub const COLOR_TEXT: Color = Color::new(0xE8, 0xEA, 0xED, 0xFF);
/// Address bar background.
pub const COLOR_ADDRESS_BG: Color = Color::new(0xF1, 0xF3, 0xF4, 0xFF);
/// Address bar text (dark).
pub const COLOR_ADDRESS_TEXT: Color = Color::new(0x1F, 0x23, 0x28, 0xFF);
/// Address bar caret.
pub const COLOR_CARET: Color = Color::new(0x1A, 0x73, 0xE8, 0xFF);
/// Dropdown background.
pub const COLOR_DROPDOWN_BG: Color = Color::new(0xFF, 0xFF, 0xFF, 0xFF);
/// Dropdown selected row background.
pub const COLOR_DROPDOWN_SELECTED: Color = Color::new(0xE8, 0xF0, 0xFE, 0xFF);
/// Dropdown hovered/active row background.
pub const COLOR_DROPDOWN_ACTIVE: Color = Color::new(0xD2, 0xE3, 0xFC, 0xFF);
/// Dropdown border.
pub const COLOR_BORDER: Color = Color::new(0xDA, 0xDC, 0xE0, 0xFF);
/// Dropdown scroll indicator.
pub const COLOR_SCROLL: Color = Color::new(0x5F, 0x63, 0x68, 0xFF);
/// Per-tab loading indicator (deterministic dot while the render worker runs).
pub const COLOR_LOADING: Color = Color::new(0x8A, 0xB4, 0xF8, 0xFF);
/// Height of one overflow-menu row, in CSS pixels.
pub const OVERFLOW_MENU_ROW_HEIGHT: f32 = 18.0;
/// Maximum open-tab rows in the overflow menu before it scrolls.
pub const OVERFLOW_MENU_MAX_VISIBLE: usize = 8;
/// Height of the link-hover status bar at the bottom of the window, in CSS pixels.
pub const STATUS_BAR_HEIGHT: f32 = 20.0;
/// Width of the context menu, in CSS pixels.
pub const CONTEXT_MENU_WIDTH: f32 = 220.0;
/// Height of one context-menu row, in CSS pixels.
pub const CONTEXT_MENU_ROW_HEIGHT: f32 = 20.0;
/// Link-hover status bar background.
pub const COLOR_STATUS_BAR: Color = Color::new(0x20, 0x21, 0x24, 0xFF);
/// Link-hover status text.
pub const COLOR_STATUS_TEXT: Color = Color::new(0xE8, 0xEA, 0xED, 0xFF);
/// Context menu background.
pub const COLOR_MENU_BG: Color = Color::new(0xFA, 0xFA, 0xFA, 0xFF);
/// Context menu selected row.
pub const COLOR_MENU_SELECTED: Color = Color::new(0xD2, 0xE3, 0xFC, 0xFF);

/// Address bar edition state: text, caret, selection anchor and an **internal process
/// clipboard** (Ctrl+C/X/V copy within the shell; the system clipboard is not touched).
///
/// `Ctrl+L` opens the bar with the whole URL selected, so typing replaces it. The caret is
/// painted as a solid deterministic bar (no time-based blinking, so screenshots are stable).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddressBar {
    text: String,
    caret: usize,
    anchor: usize,
    editing: bool,
    clipboard: String,
}

impl AddressBar {
    /// Creates a closed address bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens edition pre-filled with `url` and selects the whole URL (Ctrl+L behaviour).
    pub fn open(&mut self, url: &str) {
        self.text = url.to_string();
        self.caret = self.text.len();
        self.anchor = 0;
        self.editing = true;
    }

    /// Closes edition (cancelling any unsubmitted text; the internal clipboard survives).
    pub fn cancel(&mut self) {
        self.editing = false;
        self.text.clear();
        self.caret = 0;
        self.anchor = 0;
    }

    /// Whether the address bar is being edited.
    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    /// Current text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Caret byte offset.
    pub const fn caret(&self) -> usize {
        self.caret
    }

    /// Selection as a byte range (`start..end`), empty when collapsed.
    pub fn selection_range(&self) -> std::ops::Range<usize> {
        let start = self.anchor.min(self.caret);
        let end = self.anchor.max(self.caret);
        start..end
    }

    /// Whether a non-empty selection exists.
    pub fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    /// Selected text.
    pub fn selected_text(&self) -> &str {
        let range = self.selection_range();
        &self.text[range]
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    fn delete_selection(&mut self) -> bool {
        let range = self.selection_range();
        if range.is_empty() {
            return false;
        }
        self.text.replace_range(range.clone(), "");
        self.caret = range.start;
        self.anchor = self.caret;
        true
    }

    /// Inserts text, replacing the selection (caret ends after the inserted text).
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.delete_selection();
        self.text.insert_str(self.caret, text);
        self.caret += text.len();
        self.anchor = self.caret;
        true
    }

    /// Deletes the character before the caret (or the selection).
    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let index = prev_char_index(&self.text, self.caret);
        self.text.replace_range(index..self.caret, "");
        self.caret = index;
        self.anchor = index;
        true
    }

    /// Deletes the character after the caret (or the selection).
    pub fn delete_forward(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret >= self.text.len() {
            return false;
        }
        let next = next_char_index(&self.text, self.caret);
        self.text.replace_range(self.caret..next, "");
        true
    }

    /// Deletes the word before the caret (`Ctrl+Backspace`), or the selection.
    pub fn delete_word_back(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let mut index = self.caret;
        // Skip trailing whitespace, then the whole word.
        while index > 0 {
            let previous = prev_char_index(&self.text, index);
            if !self.text[previous..index].chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        while index > 0 {
            let previous = prev_char_index(&self.text, index);
            if self.text[previous..index].chars().all(char::is_whitespace) {
                break;
            }
            index = previous;
        }
        if index == self.caret {
            return false;
        }
        self.text.replace_range(index..self.caret, "");
        self.caret = index;
        self.anchor = index;
        true
    }

    /// Moves the caret one character left/right; without `extend` the selection collapses.
    pub fn move_caret(&mut self, right: bool, extend: bool) -> bool {
        if !extend && let Some(edge) = self.selection_edge(right) {
            self.caret = edge;
            self.anchor = edge;
            return true;
        }
        let next = if right {
            if self.caret >= self.text.len() {
                return false;
            }
            next_char_index(&self.text, self.caret)
        } else {
            if self.caret == 0 {
                return false;
            }
            prev_char_index(&self.text, self.caret)
        };
        self.caret = next;
        if !extend {
            self.anchor = next;
        }
        true
    }

    fn selection_edge(&self, right: bool) -> Option<usize> {
        if !self.has_selection() {
            return None;
        }
        let range = self.selection_range();
        Some(if right { range.end } else { range.start })
    }

    /// Moves the caret to the start/end of the text (Home/End), extending the selection
    /// when `extend` is set.
    pub fn move_home(&mut self, extend: bool) -> bool {
        let target = 0;
        if self.caret == target && !self.has_selection() {
            return false;
        }
        self.caret = target;
        if !extend {
            self.anchor = target;
        }
        true
    }

    /// Moves the caret to the end of the text.
    pub fn move_end(&mut self, extend: bool) -> bool {
        let target = self.text.len();
        if self.caret == target && !self.has_selection() {
            return false;
        }
        self.caret = target;
        if !extend {
            self.anchor = target;
        }
        true
    }

    /// Copies the selection into the internal clipboard.
    pub fn copy(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        self.clipboard = self.selected_text().to_string();
        true
    }

    /// Cuts the selection into the internal clipboard.
    pub fn cut(&mut self) -> bool {
        if !self.copy() {
            return false;
        }
        self.delete_selection()
    }

    /// Pastes the internal clipboard, replacing the selection.
    pub fn paste(&mut self) -> bool {
        if self.clipboard.is_empty() {
            return false;
        }
        let clipboard = self.clipboard.clone();
        self.insert(&clipboard)
    }

    /// Internal clipboard contents (documented: not the system clipboard).
    pub fn clipboard(&self) -> &str {
        &self.clipboard
    }

    /// First unique history URL that starts with the current text (case-insensitive, longer
    /// than the text, at least 2 chars typed). Returns the full completion candidate.
    pub fn autocomplete(&self, history: &[String]) -> Option<String> {
        let typed = self.text.trim();
        if typed.chars().count() < 2 {
            return None;
        }
        let needle = typed.to_ascii_lowercase();
        history
            .iter()
            .find(|url| {
                // A completion must extend the typed text: `contains` matched in the middle
                // ("exa" vs "https://example.com/") and `accept_completion` then selected
                // garbage (`ps://example.com/`, leaving "httm").
                url.len() > typed.len() && url.to_ascii_lowercase().starts_with(&needle)
            })
            .cloned()
    }

    /// Accepts an autocomplete candidate (Tab): completes the text and selects the suffix.
    pub fn accept_completion(&mut self, history: &[String]) -> bool {
        let Some(candidate) = self.autocomplete(history) else {
            return false;
        };
        let typed = self.text.len();
        self.text = candidate;
        self.anchor = typed;
        self.caret = self.text.len();
        true
    }

    /// Closes edition and returns whether it was open.
    pub fn finish(&mut self) -> bool {
        let was = self.editing;
        self.editing = false;
        self.anchor = self.caret;
        was
    }
}

/// Previous UTF-8 char boundary before `index`.
fn prev_char_index(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map_or(0, |(offset, _)| offset)
}

/// Next UTF-8 char boundary after `index`.
fn next_char_index(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map_or(text.len(), |character| index + character.len_utf8())
}

/// Resolves address-bar text into a URL: absolute URLs are used as-is and everything else is
/// resolved against `base` with [`net::Url::join`] (no search engine).
pub fn resolve_address(input: &str, base: Option<&net::Url>) -> Result<net::Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty address".to_string());
    }
    if let Ok(url) = net::Url::parse(input) {
        return Ok(url);
    }
    match base {
        Some(base) => base
            .join(input)
            .map_err(|error| format!("cannot resolve address `{input}`: {error}")),
        None => Err(format!("cannot parse address `{input}` as a URL")),
    }
}

/// Rectangle + title of one tab in the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct TabRect {
    /// Tab index.
    pub index: usize,
    /// Tab body rectangle (chrome CSS coordinates).
    pub rect: Rect,
    /// Close affordance rectangle.
    pub close: Rect,
    /// Compact title shown in the tab.
    pub title: String,
}

/// Computed tab strip layout.
#[derive(Clone, Debug, PartialEq)]
pub struct TabStripLayout {
    /// Visible tabs, left to right.
    pub tabs: Vec<TabRect>,
    /// New-tab `+` button.
    pub new_tab: Rect,
    /// `+N` overflow marker (when tabs do not fit).
    pub overflow: Option<Rect>,
    /// Number of hidden tabs.
    pub hidden: usize,
    /// Whether the active tab is visible.
    pub active_visible: bool,
}

/// Hit on the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabHit {
    /// Activate the tab at this index.
    Activate(usize),
    /// Close the tab at this index.
    Close(usize),
    /// Open a new tab.
    NewTab,
    /// The `+N` overflow marker was clicked (no-op: no visual overflow menu).
    Overflow,
}

/// Truncates `text` to roughly fit `max_chars` characters, adding `…`.
pub fn truncate_title(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Computes the tab strip layout for `titles` in a strip of `width` CSS px.
pub fn layout_tabs(titles: &[String], width: f32, active: usize) -> TabStripLayout {
    let width = width.max(1.0);
    let available = (width - NEW_TAB_WIDTH).max(0.0);
    let count = titles.len().max(1);
    let mut hidden = 0usize;
    let mut visible = count;
    let mut overflow_width = 0.0;
    let per_tab = if count == 0 {
        TAB_MIN_WIDTH
    } else {
        (available / count as f32).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
    };
    if per_tab * count as f32 > available && available >= TAB_MIN_WIDTH {
        let overflow = if available >= TAB_MIN_WIDTH + TAB_OVERFLOW_WIDTH {
            TAB_OVERFLOW_WIDTH
        } else {
            0.0
        };
        let fit = ((available - overflow) / TAB_MIN_WIDTH).floor().max(1.0) as usize;
        visible = fit.min(count);
        hidden = count - visible;
        if hidden > 0 {
            overflow_width = overflow;
        }
    }
    let tab_width = if visible == 0 {
        TAB_MIN_WIDTH
    } else {
        ((available - overflow_width) / visible as f32).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
    };
    let mut cursor = 0.0f32;
    let mut tabs = Vec::with_capacity(visible);
    for index in 0..visible.min(titles.len()) {
        let rect = Rect::new(cursor, 0.0, tab_width, TAB_STRIP_HEIGHT);
        let close = Rect::new(
            (cursor + tab_width - TAB_CLOSE_WIDTH - 2.0).max(cursor),
            7.0,
            TAB_CLOSE_WIDTH,
            TAB_CLOSE_WIDTH,
        );
        let max_chars = ((tab_width - TAB_CLOSE_WIDTH - 8.0) / 7.0).max(4.0) as usize;
        tabs.push(TabRect {
            index,
            rect,
            close,
            title: truncate_title(
                titles.get(index).map(String::as_str).unwrap_or(""),
                max_chars,
            ),
        });
        cursor += tab_width;
    }
    let overflow = (hidden > 0).then(|| {
        Rect::new(
            cursor,
            0.0,
            overflow_width.max(TAB_OVERFLOW_WIDTH),
            TAB_STRIP_HEIGHT,
        )
    });
    if overflow.is_some() {
        cursor += overflow_width.max(TAB_OVERFLOW_WIDTH);
    }
    let new_tab = Rect::new(cursor, 0.0, NEW_TAB_WIDTH, TAB_STRIP_HEIGHT);
    let active_visible = active < visible;
    TabStripLayout {
        tabs,
        new_tab,
        overflow,
        hidden,
        active_visible,
    }
}

/// Hit-tests the tab strip.
pub fn tab_strip_hit(layout: &TabStripLayout, x: f32, y: f32) -> Option<TabHit> {
    if !(0.0..TAB_STRIP_HEIGHT).contains(&y) {
        return None;
    }
    if let Some(overflow) = layout.overflow
        && contains(overflow, x, y)
    {
        return Some(TabHit::Overflow);
    }
    if contains(layout.new_tab, x, y) {
        return Some(TabHit::NewTab);
    }
    for tab in &layout.tabs {
        if contains(tab.close, x, y) {
            return Some(TabHit::Close(tab.index));
        }
        if contains(tab.rect, x, y) {
            return Some(TabHit::Activate(tab.index));
        }
    }
    None
}

/// Address bar rectangle for a chrome width.
pub fn address_bar_rect(width: f32) -> Rect {
    Rect::new(
        4.0,
        TAB_STRIP_HEIGHT + 2.0,
        (width - 8.0).max(1.0),
        ADDRESS_BAR_HEIGHT - 4.0,
    )
}

/// Whether a chrome CSS point is inside the chrome band.
pub fn in_chrome(x: f32, y: f32, width: f32) -> bool {
    (0.0..CHROME_HEIGHT).contains(&y) && (0.0..=width).contains(&x)
}

/// Whether a chrome CSS point is inside `rect` (public helper for menus).
pub fn contains_rect(rect: Rect, x: f32, y: f32) -> bool {
    contains(rect, x, y)
}

fn contains(rect: Rect, x: f32, y: f32) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// Compute layout of the floating `<select>` dropdown.
///
/// The list is anchored below the control (or above it when there is no room below), capped
/// to [`DROPDOWN_MAX_VISIBLE`] rows; `scroll` selects the first visible option.
pub fn dropdown_layout(
    control: &CssRect,
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
    control: &CssRect,
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
pub fn dropdown_items(options: &[SelectOption]) -> Vec<DropdownItem> {
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

/// Open overflow menu for hidden tabs (`+N` marker).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OverflowMenu {
    /// Hidden tab indices, in tab order.
    pub items: Vec<usize>,
    /// First visible row.
    pub scroll: usize,
}

impl OverflowMenu {
    /// Opens the menu for the given hidden tab indices.
    pub fn open(items: Vec<usize>) -> Self {
        Self { items, scroll: 0 }
    }

    /// Scrolls the menu rows by `direction`.
    pub fn scroll_by(&mut self, direction: i32) -> bool {
        let max = self.items.len().saturating_sub(OVERFLOW_MENU_MAX_VISIBLE);
        let next = (self.scroll as i32 + direction).clamp(0, max as i32) as usize;
        let changed = next != self.scroll;
        self.scroll = next;
        changed
    }
}

/// Overflow menu geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct OverflowLayout {
    /// Menu rectangle (chrome CSS coordinates, below the tab strip).
    pub rect: Rect,
    /// Rows (one per hidden tab).
    pub rows: Vec<Rect>,
    /// Close affordance rectangles per row.
    pub close: Vec<Rect>,
    /// First visible menu item index.
    pub first: usize,
    /// Number of visible rows.
    pub visible: usize,
}

/// Hit on the overflow menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowHit {
    /// Activate the tab at this index.
    Activate(usize),
    /// Close the tab at this index.
    Close(usize),
}

/// Computes the overflow menu layout anchored under the `+N` marker.
pub fn overflow_menu_layout(
    strip: &TabStripLayout,
    titles: &[String],
    menu_width: f32,
    viewport_height: u32,
    menu: &OverflowMenu,
) -> OverflowLayout {
    let visible = menu.items.len().min(OVERFLOW_MENU_MAX_VISIBLE);
    let height = visible as f32 * OVERFLOW_MENU_ROW_HEIGHT + 4.0;
    let anchor_x = strip.overflow.map_or(
        (strip
            .tabs
            .last()
            .map_or(0.0, |tab| tab.rect.x + tab.rect.width))
        .min(0.0),
        |rect| rect.x,
    );
    let width = menu_width.clamp(160.0, 320.0);
    let x = anchor_x.max(0.0).min((menu_width - width).max(0.0));
    let y = TAB_STRIP_HEIGHT + 2.0;
    let y = if y + height <= viewport_height as f32 {
        y
    } else {
        (viewport_height as f32 - height).max(TAB_STRIP_HEIGHT)
    };
    let rect = Rect::new(x, y, width, height);
    let mut rows = Vec::with_capacity(visible);
    let mut close = Vec::with_capacity(visible);
    for row in 0..visible {
        let index = menu.scroll + row;
        let Some(tab_index) = menu.items.get(index) else {
            break;
        };
        let _ = tab_index;
        let row_rect = Rect::new(
            rect.x + 1.0,
            rect.y + 2.0 + row as f32 * OVERFLOW_MENU_ROW_HEIGHT,
            rect.width - 2.0,
            OVERFLOW_MENU_ROW_HEIGHT,
        );
        rows.push(row_rect);
        close.push(Rect::new(
            row_rect.x + row_rect.width - 18.0,
            row_rect.y + 3.0,
            14.0,
            12.0,
        ));
        let _ = titles;
    }
    OverflowLayout {
        rect,
        rows,
        close,
        first: menu.scroll,
        visible,
    }
}

/// Hit-tests the overflow menu.
pub fn overflow_menu_hit(
    layout: &OverflowLayout,
    menu: &OverflowMenu,
    x: f32,
    y: f32,
) -> Option<OverflowHit> {
    for (row, rect) in layout.rows.iter().enumerate() {
        if contains(*rect, x, y) {
            let tab_index = *menu.items.get(layout.first + row)?;
            if contains(layout.close[row], x, y) {
                return Some(OverflowHit::Close(tab_index));
            }
            return Some(OverflowHit::Activate(tab_index));
        }
    }
    None
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
    pub control_rect: CssRect,
    /// Select options.
    pub options: Vec<SelectOption>,
    /// First visible option.
    pub scroll: usize,
    /// Hovered/active option (highlighted).
    pub active: Option<usize>,
}

impl DropdownState {
    /// Opens a dropdown for `control`.
    pub fn open(form_id: u32, control_rect: CssRect, options: Vec<SelectOption>) -> Self {
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
        let mut items = Vec::new();
        if target_link.is_some() {
            items.push(ContextMenuItem {
                action: MenuAction::OpenLinkInNewTab,
                label: "Open link in new tab",
            });
            items.push(ContextMenuItem {
                action: MenuAction::CopyLinkAddress,
                label: "Copy link address",
            });
        }
        if target_image.is_some() {
            items.push(ContextMenuItem {
                action: MenuAction::SaveImage,
                label: "Save image",
            });
            items.push(ContextMenuItem {
                action: MenuAction::CopyImage,
                label: "Copy image (not supported)",
            });
        }
        items.push(ContextMenuItem {
            action: MenuAction::Reload,
            label: "Reload",
        });
        items.push(ContextMenuItem {
            action: MenuAction::SavePagePng,
            label: "Save page as PNG",
        });
        items.push(ContextMenuItem {
            action: MenuAction::SavePageHtml,
            label: "Save page as HTML",
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

/// Rectangle of the link-hover status bar.
pub fn status_bar_rect(css_width: f32, css_height: u32) -> Rect {
    Rect::new(
        0.0,
        (css_height as f32 - STATUS_BAR_HEIGHT).max(0.0),
        css_width.max(1.0),
        STATUS_BAR_HEIGHT,
    )
}

/// Truncates a hover URL to fit the status bar width.
pub fn status_text(url: &str, css_width: f32) -> String {
    let max_chars = ((css_width - 12.0) / 6.6).max(8.0) as usize;
    truncate_title(url, max_chars)
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

/// Size of one navigation button, in CSS pixels.
pub const NAV_BUTTON_SIZE: f32 = 22.0;
/// Gap between navigation buttons, in CSS pixels.
pub const NAV_BUTTON_GAP: f32 = 2.0;

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

/// All data the chrome painter needs (plain references, no shell state).
#[derive(Clone, Copy, Debug)]
pub struct ChromeView<'a> {
    /// Tab titles, in order.
    pub tabs: &'a [String],
    /// Per-tab favicon (decoded 16x16) or `None` for the deterministic dot fallback.
    pub favicons: &'a [Option<crate::favicon::Favicon>],
    /// Active tab index.
    pub active_tab: usize,
    /// Address bar state.
    pub address: &'a AddressBar,
    /// URL shown when the address bar is not being edited.
    pub fallback_url: &'a str,
    /// Open select dropdown, if any.
    pub dropdown: Option<&'a DropdownState>,
    /// Vertical page scroll offset in CSS pixels.
    pub scroll_y: u32,
    /// Page CSS viewport height.
    pub css_viewport_height: u32,
    /// Chrome width in CSS pixels.
    pub css_width: f32,
    /// Autocomplete candidate shown as an inline gray suffix while editing.
    pub completion: Option<&'a str>,
    /// Active tab is waiting for a render (loading indicator).
    pub loading: bool,
    /// Open overflow menu for hidden tabs.
    pub overflow: Option<&'a OverflowMenu>,
    /// Hovered link URL shown in the bottom status bar.
    pub hover_url: Option<&'a str>,
    /// Open context menu.
    pub menu: Option<&'a ContextMenu>,
    /// Back navigation enabled (history has a previous entry).
    pub can_go_back: bool,
    /// Forward navigation enabled.
    pub can_go_forward: bool,
    /// Open Ctrl+H history menu.
    pub history_menu: Option<&'a crate::history::HistoryMenu>,
    /// Download progress text (takes precedence over the hover URL in the status bar).
    pub download_status: Option<&'a str>,
    /// Show the visual history button in the address bar (window only; headless off keeps
    /// the existing chrome goldens byte-identical).
    pub history_button: bool,
}

/// Paints the chrome over a physical `0RGB` framebuffer; returns the painted pixel count.
///
/// `scale` maps CSS pixels to physical pixels (effective device scale × page zoom).
pub fn draw_chrome(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    scale: f32,
    view: &ChromeView<'_>,
) -> usize {
    if dest_width == 0
        || dest_height == 0
        || dest.len() < (dest_width as usize) * (dest_height as usize)
    {
        return 0;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pixmap = Pixmap::new(dest_width, dest_height);
    let css_width = view.css_width.max(1.0);
    paint_tab_strip(&mut pixmap, scale, view, css_width);
    paint_address_bar(&mut pixmap, scale, view, css_width);
    if let Some(dropdown) = view.dropdown {
        paint_dropdown(&mut pixmap, scale, view, dropdown);
    }
    paint_navigation_buttons(&mut pixmap, scale, view);
    paint_history_button(&mut pixmap, scale, view);
    if let Some(menu) = view.history_menu {
        paint_history_menu(&mut pixmap, scale, view, menu);
    }
    if let Some(menu) = view.overflow {
        paint_overflow_menu(&mut pixmap, scale, view, menu);
    }
    if let Some(text) = view.download_status.or(view.hover_url) {
        paint_status_bar(&mut pixmap, scale, view, text);
    }
    if let Some(menu) = view.menu {
        paint_context_menu(&mut pixmap, scale, view, menu);
    }
    let mut painted = 0usize;
    for (index, color) in pixmap.pixels.iter().enumerate() {
        if color.a == 0 {
            continue;
        }
        let base = dest[index];
        let alpha = f32::from(color.a) / 255.0;
        let mix = |channel: u32, target: u8| -> u32 {
            let value = channel as f32 * (1.0 - alpha) + f32::from(target) * alpha;
            value.round().clamp(0.0, 255.0) as u32
        };
        dest[index] = (mix(base >> 16 & 0xFF, color.r) << 16)
            | (mix(base >> 8 & 0xFF, color.g) << 8)
            | mix(base & 0xFF, color.b);
        painted += 1;
    }
    painted
}

fn paint_tab_strip(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, width: f32) {
    pixmap.fill_rect(
        Rect::new(0.0, 0.0, width * scale, TAB_STRIP_HEIGHT * scale),
        COLOR_TAB_STRIP,
    );
    let layout = layout_tabs(view.tabs, width, view.active_tab);
    for tab in &layout.tabs {
        let color = if tab.index == view.active_tab {
            COLOR_TAB_ACTIVE
        } else {
            COLOR_TAB
        };
        pixmap.fill_rect(scaled(tab.rect, scale), color);
        let mut title_x = tab.rect.x + 6.0;
        if let Some(Some(icon)) = view.favicons.get(tab.index) {
            title_x = tab.rect.x + 23.0;
            if let Some(pixels) = icon.pixels {
                for y in 0..crate::favicon::ICON_SIZE {
                    for x in 0..crate::favicon::ICON_SIZE {
                        let offset = ((y * crate::favicon::ICON_SIZE + x) * 4) as usize;
                        let alpha = pixels[offset + 3];
                        if alpha == 0 {
                            continue;
                        }
                        pixmap.fill_rect(
                            Rect::new(
                                (tab.rect.x + 3.0 + x as f32) * scale,
                                (tab.rect.y + 4.0 + y as f32) * scale,
                                scale,
                                scale,
                            ),
                            Color::new(
                                pixels[offset],
                                pixels[offset + 1],
                                pixels[offset + 2],
                                alpha,
                            ),
                        );
                    }
                }
            } else {
                pixmap.fill_rect(
                    Rect::new(
                        (tab.rect.x + 5.0) * scale,
                        (tab.rect.y + 8.0) * scale,
                        8.0 * scale,
                        8.0 * scale,
                    ),
                    Color::new(icon.color[0], icon.color[1], icon.color[2], 0xFF),
                );
            }
        }
        draw_text(
            pixmap,
            &tab.title,
            title_x,
            tab.rect.y + 8.0,
            scale,
            11.0,
            COLOR_TEXT,
        );
        if view.loading && tab.index == view.active_tab {
            pixmap.fill_rect(
                Rect::new(
                    (tab.rect.x + 3.0) * scale,
                    (tab.rect.y + 12.0) * scale,
                    4.0 * scale,
                    4.0 * scale,
                ),
                COLOR_LOADING,
            );
        }
        let close = tab.close;
        draw_text(
            pixmap,
            "x",
            close.x + 3.0,
            close.y + 3.0,
            scale,
            10.0,
            COLOR_TEXT,
        );
    }
    if let Some(overflow) = layout.overflow {
        pixmap.fill_rect(scaled(overflow, scale), COLOR_TAB_STRIP);
        draw_text(
            pixmap,
            &format!("+{}", layout.hidden),
            overflow.x + 6.0,
            overflow.y + 8.0,
            scale,
            11.0,
            COLOR_TEXT,
        );
    }
    let new_tab = layout.new_tab;
    draw_text(
        pixmap,
        "+",
        new_tab.x + 9.0,
        new_tab.y + 8.0,
        scale,
        14.0,
        COLOR_TEXT,
    );
}

fn paint_address_bar(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, width: f32) {
    let bar = address_bar_rect(width);
    pixmap.fill_rect(
        Rect::new(
            0.0,
            TAB_STRIP_HEIGHT * scale,
            width * scale,
            ADDRESS_BAR_HEIGHT * scale,
        ),
        Color::new(0xDE, 0xE1, 0xE6, 0xFF),
    );
    pixmap.fill_rect(scaled(bar, scale), COLOR_ADDRESS_BG);
    let text = if view.address.is_editing() {
        view.address.text()
    } else {
        view.fallback_url
    };
    draw_text(
        pixmap,
        text,
        bar.x + 6.0,
        bar.y + 8.0,
        scale,
        12.0,
        COLOR_ADDRESS_TEXT,
    );
    if view.address.is_editing() {
        let before = &text[..view.address.caret().min(text.len())];
        let caret_x = bar.x + 6.0 + before.chars().count() as f32 * 12.0 * 0.6;
        pixmap.fill_rect(
            Rect::new(
                caret_x * scale,
                (bar.y + 5.0) * scale,
                scale.max(1.0),
                (bar.height - 10.0) * scale,
            ),
            COLOR_CARET,
        );
    }
}

fn paint_dropdown(
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
    pixmap.fill_rect(scaled(list.rect, scale), COLOR_BORDER);
    pixmap.fill_rect(
        scaled(
            Rect::new(
                list.rect.x + 1.0,
                list.rect.y + 1.0,
                list.rect.width - 2.0,
                list.rect.height - 2.0,
            ),
            scale,
        ),
        COLOR_DROPDOWN_BG,
    );
    for (row, rect) in list.rows.iter().enumerate() {
        let item_index = list.first + row;
        let Some(item) = items.get(item_index) else {
            break;
        };
        let option = item
            .option_index
            .and_then(|index| dropdown.options.get(index));
        let background = if item.option_index.is_none() {
            Color::new(0xF5, 0xF5, 0xF5, 0xFF)
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
        pixmap.fill_rect(scaled(track, scale), Color::new(0xE0, 0xE0, 0xE0, 0xFF));
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

fn paint_overflow_menu(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    menu: &OverflowMenu,
) {
    let strip = layout_tabs(view.tabs, view.css_width, view.active_tab);
    let layout = overflow_menu_layout(
        &strip,
        view.tabs,
        view.css_width,
        view.css_viewport_height,
        menu,
    );
    pixmap.fill_rect(scaled(layout.rect, scale), COLOR_BORDER);
    pixmap.fill_rect(
        scaled(
            Rect::new(
                layout.rect.x + 1.0,
                layout.rect.y + 1.0,
                layout.rect.width - 2.0,
                layout.rect.height - 2.0,
            ),
            scale,
        ),
        Color::new(0xFA, 0xFA, 0xFA, 0xFF),
    );
    for (row, rect) in layout.rows.iter().enumerate() {
        let Some(tab_index) = menu.items.get(layout.first + row) else {
            break;
        };
        let title = view
            .tabs
            .get(*tab_index)
            .map(String::as_str)
            .unwrap_or("Tab");
        draw_text(
            pixmap,
            &truncate_title(title, ((rect.width - 24.0) / 7.0).max(4.0) as usize),
            rect.x + 5.0,
            rect.y + 5.0,
            scale,
            11.0,
            COLOR_ADDRESS_TEXT,
        );
        draw_text(
            pixmap,
            "x",
            layout.close[row].x + 3.0,
            layout.close[row].y + 3.0,
            scale,
            10.0,
            COLOR_ADDRESS_TEXT,
        );
    }
}

fn paint_status_bar(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, url: &str) {
    let rect = status_bar_rect(view.css_width, view.css_viewport_height);
    pixmap.fill_rect(scaled(rect, scale), COLOR_STATUS_BAR);
    draw_text(
        pixmap,
        &status_text(url, rect.width),
        rect.x + 6.0,
        rect.y + 6.0,
        scale,
        11.0,
        COLOR_STATUS_TEXT,
    );
}

fn paint_context_menu(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, menu: &ContextMenu) {
    let layout = context_menu_layout(menu, view.css_width, view.css_viewport_height);
    pixmap.fill_rect(scaled(layout.rect, scale), COLOR_BORDER);
    pixmap.fill_rect(
        scaled(
            Rect::new(
                layout.rect.x + 1.0,
                layout.rect.y + 1.0,
                layout.rect.width - 2.0,
                layout.rect.height - 2.0,
            ),
            scale,
        ),
        COLOR_MENU_BG,
    );
    for (index, row) in layout.rows.iter().enumerate() {
        let Some(item) = menu.items.get(index) else {
            break;
        };
        if index == menu.selected {
            pixmap.fill_rect(scaled(*row, scale), COLOR_MENU_SELECTED);
        }
        draw_text(
            pixmap,
            item.label,
            row.x + 8.0,
            row.y + 6.0,
            scale,
            11.0,
            COLOR_ADDRESS_TEXT,
        );
    }
}

/// History button rectangle (right side of the address bar); only painted when
/// [`ChromeView::history_button`] is set so headless chrome goldens stay unchanged.
pub fn history_button_rect(css_width: f32) -> Rect {
    Rect::new(
        (css_width - 28.0).max(2.0),
        TAB_STRIP_HEIGHT + 4.0,
        22.0,
        20.0,
    )
}

/// Hit-tests the visual history button.
pub fn history_button_hit(css_width: f32, x: f32, y: f32) -> bool {
    contains(history_button_rect(css_width), x, y)
}

fn paint_history_button(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>) {
    if !view.history_button {
        return;
    }
    let rect = history_button_rect(view.css_width);
    pixmap.fill_rect(scaled(rect, scale), COLOR_BORDER);
    draw_text(
        pixmap,
        "H",
        rect.x + 7.0,
        rect.y + 4.0,
        scale,
        11.0,
        COLOR_ADDRESS_TEXT,
    );
}

fn paint_navigation_buttons(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>) {
    let buttons = navigation_buttons(view.css_width);
    let mut draw = |rect: Rect, label: &str, enabled: bool| {
        let color = if enabled {
            COLOR_ADDRESS_TEXT
        } else {
            Color::new(0x9A, 0xA0, 0xA6, 0xFF)
        };
        draw_text(
            pixmap,
            label,
            rect.x + 6.0,
            rect.y + 5.0,
            scale,
            12.0,
            color,
        );
    };
    draw(buttons.back, "<", view.can_go_back);
    draw(buttons.forward, ">", view.can_go_forward);
    draw(buttons.reload, "R", true);
}

fn paint_history_menu(
    pixmap: &mut Pixmap,
    scale: f32,
    view: &ChromeView<'_>,
    menu: &crate::history::HistoryMenu,
) {
    let layout = history_menu_layout(menu, view.css_width, view.css_viewport_height);
    pixmap.fill_rect(scaled(layout.rect, scale), COLOR_BORDER);
    pixmap.fill_rect(
        scaled(
            Rect::new(
                layout.rect.x + 1.0,
                layout.rect.y + 1.0,
                layout.rect.width - 2.0,
                layout.rect.height - 2.0,
            ),
            scale,
        ),
        COLOR_MENU_BG,
    );
    if menu.entries.is_empty() {
        draw_text(
            pixmap,
            "No history yet",
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
            pixmap.fill_rect(scaled(*row, scale), COLOR_MENU_SELECTED);
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

fn scaled(rect: Rect, scale: f32) -> Rect {
    Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    )
}

fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    x_css: f32,
    y_css: f32,
    scale: f32,
    font_size_css: f32,
    color: Color,
) {
    if text.is_empty() {
        return;
    }
    pixmap.draw_text(
        text,
        Rect::new(x_css * scale, y_css * scale, 0.0, 0.0),
        color,
        font_size_css * scale,
    );
}

/// Paints the simple chrome band (tab strip + address bar) **directly over** an
/// RGBA page pixmap, avoiding the full-framebuffer `0RGB` round-trip used by
/// [`draw_chrome`] (R196).
///
/// Only the top [`CHROME_HEIGHT`] CSS pixels are touched. The caller must not
/// pass overlays (dropdown/overflow/status/context menu); the headless
/// `--chrome` path uses a plain view without them. The blend formula and the
/// opaque-alpha output match the full-framebuffer path byte for byte, so the
/// deterministic chrome screenshot bytes do not change.
pub fn draw_chrome_over_pixmap(
    dest: &mut Pixmap,
    rows: u32,
    scale: f32,
    view: &ChromeView<'_>,
) -> usize {
    let width = dest.width;
    let height = dest.height;
    if width == 0 || height == 0 || rows == 0 {
        return 0;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let rows = rows.min(height);
    // The painted surfaces only need the band they can cover; every paint helper
    // clips/fills within this height and the composite loop below mirrors the
    // full-buffer mix for exactly these rows.
    let mut band = Pixmap::new(width, rows);
    let css_width = view.css_width.max(1.0);
    paint_tab_strip(&mut band, scale, view, css_width);
    paint_address_bar(&mut band, scale, view, css_width);
    paint_navigation_buttons(&mut band, scale, view);
    paint_history_button(&mut band, scale, view);
    debug_assert!(view.dropdown.is_none());
    debug_assert!(view.overflow.is_none());
    debug_assert!(view.menu.is_none());
    debug_assert!(view.hover_url.is_none());
    let mut painted = 0usize;
    for (index, color) in band.pixels.iter().enumerate() {
        if color.a == 0 {
            continue;
        }
        if color.a == 255 {
            // The chrome surfaces are overwhelmingly opaque; assigning directly is
            // byte-identical to the alpha mix at alpha == 1.0 and skips the f32 work (R196).
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

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    fn buffer(width: u32, height: u32) -> Vec<u32> {
        vec![0x00FF_FFFF; (width * height) as usize]
    }

    #[test]
    fn address_bar_selects_all_on_open_and_edits() {
        let mut bar = AddressBar::new();
        assert!(!bar.is_editing());
        bar.open("https://example.com/a");
        assert!(bar.is_editing());
        assert!(bar.has_selection(), "Ctrl+L selects the URL");
        assert_eq!(bar.selection_range(), 0.."https://example.com/a".len());
        // Typing replaces the selection.
        assert!(bar.insert("x"));
        assert_eq!(bar.text(), "x");
        assert!(!bar.has_selection());
        bar.insert("yz");
        assert_eq!(bar.text(), "xyz");
        assert!(bar.backspace());
        assert_eq!(bar.text(), "xy");
        assert!(!bar.delete_forward(), "caret at end");
        assert_eq!(bar.text(), "xy");
        bar.move_home(false);
        assert_eq!(bar.caret(), 0);
        assert!(bar.delete_forward());
        assert_eq!(bar.text(), "y");
        bar.move_end(false);
        assert_eq!(bar.caret(), bar.text().len());
        assert!(bar.finish());
        assert!(!bar.is_editing());
    }

    #[test]
    fn address_bar_selection_copy_cut_paste_and_word_delete() {
        let mut bar = AddressBar::new();
        bar.open("one two three");
        bar.select_all();
        assert_eq!(bar.selected_text(), "one two three");
        assert!(bar.copy());
        assert_eq!(bar.clipboard(), "one two three");
        bar.move_end(false);
        bar.delete_word_back();
        assert_eq!(bar.text(), "one two ");
        bar.delete_word_back();
        assert_eq!(bar.text(), "one ");
        bar.delete_word_back();
        assert_eq!(bar.text(), "");
        assert!(bar.paste());
        assert_eq!(bar.text(), "one two three");
        // Cut the selection, then paste it back.
        bar.select_all();
        assert!(bar.cut());
        assert_eq!(bar.text(), "");
        assert!(bar.paste());
        assert_eq!(bar.text(), "one two three");
        // Shift+arrow extends selection, Delete removes it.
        bar.move_home(false);
        bar.move_caret(true, true);
        bar.move_caret(true, true);
        assert!(bar.has_selection());
        bar.delete_forward();
        assert_eq!(bar.text(), "e two three");
    }

    #[test]
    fn address_bar_autocomplete_needs_two_chars_and_accepts_suffix() {
        let mut bar = AddressBar::new();
        bar.open("");
        bar.insert("h");
        let history = vec![
            "https://example.com/hello".to_string(),
            "https://example.com/help".to_string(),
        ];
        assert_eq!(bar.autocomplete(&history), None, "min 2 chars");
        bar.insert("e");
        assert_eq!(
            bar.autocomplete(&history).as_deref(),
            Some("https://example.com/hello")
        );
        assert!(bar.accept_completion(&history));
        assert_eq!(bar.text(), "https://example.com/hello");
        assert!(bar.has_selection(), "completion suffix selected");
    }

    #[test]
    fn resolve_address_parses_absolute_and_joins_relative() {
        let base = net::Url::parse("https://example.com/dir/page.html").expect("base");
        assert_eq!(
            resolve_address("https://other.example/x", Some(&base))
                .expect("absolute")
                .as_str(),
            "https://other.example/x"
        );
        assert_eq!(
            resolve_address("other.html", Some(&base))
                .expect("relative")
                .as_str(),
            "https://example.com/dir/other.html"
        );
        assert!(resolve_address("", Some(&base)).is_err());
        assert!(resolve_address("https://[bad", Some(&base)).is_err());
    }

    #[test]
    fn tab_strip_layout_distributes_and_reserves_buttons() {
        let titles: Vec<String> = (0..3).map(|index| format!("Tab {index}")).collect();
        let layout = layout_tabs(&titles, 640.0, 1);
        assert_eq!(layout.tabs.len(), 3);
        assert_eq!(layout.hidden, 0);
        for (index, tab) in layout.tabs.iter().enumerate() {
            assert_eq!(tab.index, index);
            assert!(tab.rect.width >= TAB_MIN_WIDTH - 0.1);
            assert!(tab.close.x >= tab.rect.x);
        }
        assert!(layout.new_tab.x >= layout.tabs.last().expect("tab").rect.x);
    }

    #[test]
    fn tab_strip_hit_tests_activate_close_and_new() {
        let titles: Vec<String> = (0..2).map(|index| format!("Tab {index}")).collect();
        let layout = layout_tabs(&titles, 400.0, 0);
        let first = &layout.tabs[0];
        assert_eq!(
            tab_strip_hit(&layout, first.rect.x + 4.0, 10.0),
            Some(TabHit::Activate(0))
        );
        assert_eq!(
            tab_strip_hit(&layout, first.close.x + 2.0, first.close.y + 2.0),
            Some(TabHit::Close(0))
        );
        assert_eq!(
            tab_strip_hit(&layout, layout.new_tab.x + 2.0, 10.0),
            Some(TabHit::NewTab)
        );
        assert_eq!(tab_strip_hit(&layout, 10.0, TAB_STRIP_HEIGHT + 2.0), None);
    }

    #[test]
    fn tab_strip_overflows_with_marker() {
        let titles: Vec<String> = (0..20).map(|index| format!("Tab {index}")).collect();
        let layout = layout_tabs(&titles, 300.0, 19);
        assert!(layout.hidden > 0, "expected hidden tabs");
        let overflow = layout.overflow.expect("overflow marker");
        assert!(overflow.width >= TAB_OVERFLOW_WIDTH - 0.1);
        assert_eq!(
            tab_strip_hit(&layout, overflow.x + 2.0, 10.0),
            Some(TabHit::Overflow)
        );
        assert!(!layout.active_visible);
    }

    #[test]
    fn dropdown_layout_caps_rows_and_scrolls() {
        let control = CssRect::new(10.0, 100.0, 120.0, 18.0);
        let list = dropdown_layout(&control, 20, 400, 0, 0);
        assert_eq!(list.visible, DROPDOWN_MAX_VISIBLE);
        assert_eq!(list.total, 20);
        assert_eq!(list.first, 0);
        assert!(list.rect.height > 0.0);
        let scrolled = dropdown_layout(&control, 20, 400, 0, 5);
        assert_eq!(scrolled.first, 5);
        assert_eq!(
            dropdown_layout(&control, 20, 400, 0, 999).first,
            20 - DROPDOWN_MAX_VISIBLE
        );
    }

    #[test]
    fn dropdown_hit_and_state_navigation() {
        let control = CssRect::new(10.0, 100.0, 120.0, 18.0);
        let options = vec![
            SelectOption::new("One", "1", true),
            SelectOption::new("Two", "2", false),
            SelectOption::new("Three", "3", false),
        ];
        let mut dropdown = DropdownState::open(7, control, options.clone());
        assert!(dropdown.is_open());
        assert_eq!(dropdown.active(), Some(0));
        assert!(dropdown.move_active(true));
        assert_eq!(dropdown.active(), Some(1));
        assert!(!dropdown.move_active(true) || dropdown.active() == Some(2));
        let list = dropdown_layout(&control, options.len(), 400, 0, 0);
        let row = list.rows[1];
        assert_eq!(list.hit_option(row.x + 2.0, row.y + 2.0), Some(1));
        assert!(list.contains_point(list.rect.x + 1.0, list.rect.y + 1.0));
        assert!(!dropdown.scroll_by(5), "nothing to scroll");
        dropdown.close();
        assert!(!dropdown.is_open());
    }

    #[test]
    fn dropdown_opens_above_when_no_room_below() {
        let control = CssRect::new(10.0, 360.0, 120.0, 18.0);
        let list = dropdown_layout(&control, 6, 400, 0, 0);
        assert!(list.above);
        assert!(list.rect.y < 360.0);
    }

    #[test]
    fn chrome_paints_tab_strip_and_address_bar() {
        let address = AddressBar::new();
        let tabs = vec!["Megi".to_string(), "Docs".to_string()];
        let mut dest = buffer(640, 400);
        let painted = draw_chrome(&mut dest, 640, 400, 1.0, &view(&address, None, &tabs));
        assert!(painted > 0);
        let pixel = |x: u32, y: u32| dest[(y * 640 + x) as usize];
        assert_eq!(pixel(620, 2), 0x0020_2124, "tab strip background");
        assert_eq!(
            pixel(100, TAB_STRIP_HEIGHT as u32 + 2),
            0x00F1_F3F4,
            "address bar"
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
        assert_eq!(pixel(first.x as u32 + 3, first.y as u32 + 3), 0x00E8_F0FE);
        assert_eq!(pixel(second.x as u32 + 3, second.y as u32 + 3), 0x00D2_E3FC);
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
            .filter(|pixel| **pixel == 0x001A_73E8)
            .count();
        assert!(caret_pixels > 0, "caret visible");
    }

    #[test]
    fn chrome_geometry_helpers() {
        assert_eq!(CHROME_HEIGHT, 58.0);
        assert!(in_chrome(10.0, 10.0, 640.0));
        assert!(!in_chrome(10.0, CHROME_HEIGHT + 1.0, 640.0));
        assert_eq!(truncate_title("Hello World", 5), "Hell…");
        assert_eq!(truncate_title("Hi", 5), "Hi");
        assert_eq!(address_bar_rect(640.0).width, 632.0);
    }

    #[test]
    fn overflow_menu_lists_hidden_tabs_and_hit_tests_activate_and_close() {
        let titles: Vec<String> = (0..12).map(|index| format!("Tab {index}")).collect();
        let strip = layout_tabs(&titles, 300.0, 0);
        assert!(strip.hidden > 0);
        let hidden: Vec<usize> = (strip.tabs.len()..titles.len()).collect();
        let menu = OverflowMenu::open(hidden.clone());
        let layout = overflow_menu_layout(&strip, &titles, 300.0, 400, &menu);
        assert_eq!(
            layout.visible,
            menu.items.len().min(OVERFLOW_MENU_MAX_VISIBLE)
        );
        let first = layout.rows[0];
        assert_eq!(
            overflow_menu_hit(&layout, &menu, first.x + 4.0, first.y + 4.0),
            Some(OverflowHit::Activate(hidden[0]))
        );
        let close = layout.close[0];
        assert_eq!(
            overflow_menu_hit(&layout, &menu, close.x + 2.0, close.y + 2.0),
            Some(OverflowHit::Close(hidden[0]))
        );
        assert_eq!(overflow_menu_hit(&layout, &menu, 1.0, 1.0), None);
    }

    #[test]
    fn dropdown_optgroup_headers_are_not_selectable() {
        let options = vec![
            SelectOption::new("Apple", "a", false).with_group(Some("Fruits".to_string())),
            SelectOption::new("Pear", "p", false).with_group(Some("Fruits".to_string())),
            SelectOption::new("Leek", "l", false).with_group(Some("Veg".to_string())),
        ];
        let items = dropdown_items(&options);
        assert_eq!(items.len(), 5, "2 group headers + 3 options");
        assert_eq!(items[0].label, "Fruits");
        assert!(!items[0].is_option());
        assert_eq!(items[1].option_index, Some(0));
        let control = CssRect::new(10.0, 40.0, 120.0, 18.0);
        let list = dropdown_layout_items(&control, &items, 400, 0, 0);
        assert_eq!(
            list.hit_option(list.rows[0].x + 2.0, list.rows[0].y + 2.0),
            None
        );
        assert_eq!(
            list.hit_option(list.rows[1].x + 2.0, list.rows[1].y + 2.0),
            Some(0)
        );
    }

    #[test]
    fn dropdown_keyboard_home_end_page_and_first_char_skip_headers() {
        let options = vec![
            SelectOption::new("Apple", "a", false).with_group(Some("Fruits".to_string())),
            SelectOption::new("Pear", "p", false).with_group(Some("Fruits".to_string())),
            SelectOption::new("Leek", "l", false).with_group(Some("Veg".to_string())),
        ];
        let control = CssRect::new(10.0, 40.0, 120.0, 18.0);
        let mut dropdown = DropdownState::open(1, control, options);
        assert!(dropdown.move_home());
        assert_eq!(
            dropdown.active(),
            Some(1),
            "first selectable row (after header)"
        );
        assert!(dropdown.move_page(true));
        assert_eq!(
            dropdown.active(),
            Some(4),
            "PageDown reaches the last option"
        );
        assert!(dropdown.move_page(false));
        assert_eq!(
            dropdown.active(),
            Some(1),
            "PageUp returns to the first option"
        );
        assert!(dropdown.first_char('p'));
        assert_eq!(dropdown.active(), Some(2));
        assert!(dropdown.move_end());
        assert_eq!(dropdown.active(), Some(4));
    }

    #[test]
    fn truncation_uses_an_ellipsis() {
        assert_eq!(truncate_title("abcdefghij", 5), "abcd…");
        assert_eq!(truncate_title("abc", 5), "abc");
        assert_eq!(truncate_title("", 5), "");
    }

    #[test]
    fn overflow_menu_scrolls_and_hits_after_scrolling() {
        let titles: Vec<String> = (0..20).map(|index| format!("Tab {index}")).collect();
        let strip = layout_tabs(&titles, 260.0, 0);
        let hidden: Vec<usize> = (strip.tabs.len()..titles.len()).collect();
        let mut menu = OverflowMenu::open(hidden.clone());
        let before = overflow_menu_layout(&strip, &titles, 260.0, 400, &menu);
        assert!(menu.scroll_by(1));
        let after = overflow_menu_layout(&strip, &titles, 260.0, 400, &menu);
        assert_eq!(after.first, 1);
        let item = hidden[1];
        assert_eq!(
            overflow_menu_hit(&after, &menu, after.rows[0].x + 3.0, after.rows[0].y + 3.0),
            Some(OverflowHit::Activate(item))
        );
        assert_eq!(
            before.rows[0], after.rows[0],
            "menu geometry is stable when scrolling"
        );
        let _ = before;
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
            .filter(|pixel| **pixel == 0x008A_B4F8)
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
        let scroll_pixels = dest.iter().filter(|pixel| **pixel == 0x005F_6368).count();
        assert!(scroll_pixels > 10, "scrollbar painted: {scroll_pixels}");
    }

    #[test]
    fn address_bar_home_end_and_shift_selection() {
        let mut bar = AddressBar::new();
        bar.open("abc");
        bar.select_all();
        bar.move_home(false);
        assert_eq!(bar.caret(), 0);
        assert!(!bar.has_selection());
        bar.move_end(true);
        assert!(bar.has_selection());
        assert_eq!(bar.selection_range(), 0..3);
        bar.move_home(true);
        assert_eq!(bar.selection_range(), 0..0);
        assert!(bar.insert("x"));
        assert_eq!(bar.text(), "xabc");
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
        assert_eq!(dest[bottom], 0x0020_2124, "status bar background");
        let mut clean = buffer(320, 200);
        draw_chrome(
            &mut clean,
            320,
            200,
            1.0,
            &view_without_hover(&address, &tabs),
        );
        assert_ne!(clean[bottom], 0x0020_2124, "no bar without hover");
    }

    fn view_without_hover<'a>(address: &'a AddressBar, tabs: &'a [String]) -> ChromeView<'a> {
        let mut view = view(address, None, tabs);
        view.hover_url = None;
        view
    }

    #[test]
    fn context_menu_items_depend_on_targets() {
        let page = ContextMenu::new(None, None, 10.0, 10.0);
        assert_eq!(page.items.len(), 3, "page actions");
        let link = ContextMenu::new(Some("https://example.com/a".to_string()), None, 10.0, 10.0);
        assert_eq!(link.items.len(), 5);
        assert_eq!(link.items[0].action, MenuAction::OpenLinkInNewTab);
        assert_eq!(link.items[1].action, MenuAction::CopyLinkAddress);
        let image = crate::images::ImageRegion {
            node_id: dom::NodeId::from_index(0),
            rect: crate::links::CssRect::new(0.0, 0.0, 10.0, 10.0),
            url: "https://example.com/i.png".to_string(),
        };
        let both = ContextMenu::new(
            Some("https://example.com/a".to_string()),
            Some(image),
            10.0,
            10.0,
        );
        assert_eq!(both.items.len(), 7);
        assert_eq!(both.items[2].action, MenuAction::SaveImage);
        assert_eq!(both.items[3].action, MenuAction::CopyImage);
        assert!(!MenuAction::CopyImage.is_implemented());
        assert!(MenuAction::SavePagePng.is_implemented());
    }

    #[test]
    fn context_menu_navigation_layout_and_hit() {
        let mut menu = ContextMenu::new(None, None, 10.0, 10.0);
        assert!(menu.move_selection(true));
        assert_eq!(menu.selected, 1);
        assert!(menu.move_selection(false));
        menu.move_selection(false);
        assert_eq!(menu.selected, menu.items.len() - 1, "wraps upward");
        let layout = context_menu_layout(&menu, 320.0, 200);
        assert!(layout.rect.x >= 2.0 && layout.rect.y >= 2.0);
        assert_eq!(layout.rows.len(), menu.items.len());
        let row = layout.rows[1];
        assert_eq!(context_menu_hit(&layout, row.x + 2.0, row.y + 2.0), Some(1));
        assert_eq!(context_menu_hit(&layout, 1.0, 1.0), None);
        // Near the bottom edge the menu flips above the cursor.
        let flipped = ContextMenu::new(None, None, 300.0, 195.0);
        let flipped_layout = context_menu_layout(&flipped, 320.0, 200);
        assert!(flipped_layout.rect.y + flipped_layout.rect.height <= 200.0);
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
        let pixel = dest[(selected.y as u32 * 400 + selected.x as u32 + 3) as usize];
        assert_eq!(pixel, 0x00D2_E3FC, "selected row background");
        assert!(selected.width > 0.0);
    }

    #[test]
    fn navigation_buttons_layout_hit_and_disabled_state() {
        let buttons = navigation_buttons(640.0);
        assert_eq!(buttons.back.width, NAV_BUTTON_SIZE);
        assert!(buttons.forward.x > buttons.back.x);
        assert_eq!(
            navigation_button_at(
                &buttons,
                buttons.back.x + 2.0,
                buttons.back.y + 2.0,
                true,
                false
            ),
            Some(NavigationButton::Back)
        );
        assert_eq!(
            navigation_button_at(
                &buttons,
                buttons.back.x + 2.0,
                buttons.back.y + 2.0,
                false,
                false
            ),
            None,
            "disabled back button is not clickable"
        );
        assert_eq!(
            navigation_button_at(
                &buttons,
                buttons.forward.x + 2.0,
                buttons.forward.y + 2.0,
                false,
                true
            ),
            Some(NavigationButton::Forward)
        );
        assert_eq!(
            navigation_button_at(
                &buttons,
                buttons.reload.x + 2.0,
                buttons.reload.y + 2.0,
                false,
                false
            ),
            Some(NavigationButton::Reload),
            "reload is always enabled"
        );
        assert_eq!(navigation_button_at(&buttons, 1.0, 1.0, true, true), None);
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
                if dest[y * 480 + x] != 0x00F1_F3F4 && dest[y * 480 + x] != 0x00DE_E1E6 {
                    dark += 1;
                }
            }
        }
        assert!(dark > 0, "reload glyph painted");
    }

    #[test]
    fn history_menu_layout_hit_and_draw() {
        let entries = vec![
            crate::history::HistoryEntry {
                title: "A".to_string(),
                url: "https://example.com/a".to_string(),
                logical_time: 1,
            },
            crate::history::HistoryEntry {
                title: "B".to_string(),
                url: "https://example.com/b".to_string(),
                logical_time: 2,
            },
        ];
        let menu = crate::history::HistoryMenu::open(entries, 40.0, 60.0);
        let layout = history_menu_layout(&menu, 480.0, 300);
        assert_eq!(layout.rows.len(), 2);
        assert_eq!(
            history_menu_hit(&layout, layout.rows[1].x + 2.0, layout.rows[1].y + 2.0),
            Some(1)
        );
        assert_eq!(history_menu_hit(&layout, 1.0, 1.0), None);
        let address = AddressBar::new();
        let tabs = vec!["Megi".to_string()];
        let mut dest = buffer(480, 300);
        let mut view = view(&address, None, &tabs);
        view.history_menu = Some(&menu);
        draw_chrome(&mut dest, 480, 300, 1.0, &view);
        let selected = layout.rows[0];
        assert_eq!(
            dest[(selected.y as u32 * 480 + selected.x as u32 + 3) as usize],
            0x00D2_E3FC,
            "selected history row"
        );
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
        assert_eq!(dest[bottom], 0x0020_2124, "status bar painted for download");
        let without = view_without_download(&address, &tabs);
        assert_eq!(without.download_status, None);
    }

    fn view_without_download<'a>(address: &'a AddressBar, tabs: &'a [String]) -> ChromeView<'a> {
        let mut view = view(address, None, tabs);
        view.download_status = None;
        view
    }

    #[test]
    fn history_button_geometry_and_hit_testing() {
        let rect = history_button_rect(640.0);
        assert!(rect.x + rect.width <= 640.0);
        assert!(history_button_hit(640.0, rect.x + 2.0, rect.y + 2.0));
        assert!(!history_button_hit(640.0, 1.0, 1.0));
        let address = AddressBar::new();
        let tabs = vec!["Megi".to_string()];
        let mut view = view(&address, None, &tabs);
        view.history_button = true;
        assert!(view.history_button);
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
        let expected =
            (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2]);
        // The badge is drawn a few pixels inside the row; scan for its color.
        let start = (row.y as u32 + 4) * 480 + row.x as u32 + 4;
        assert!(
            dest[start as usize..(start + 30) as usize].contains(&expected),
            "domain badge color painted"
        );
    }
    #[test]
    fn autocomplete_requires_a_prefix_and_selects_only_the_suffix() {
        let history = vec!["https://example.com/".to_string()];
        let mut bar = AddressBar::new();
        bar.open("");
        bar.insert("exa");
        assert_eq!(
            bar.autocomplete(&history),
            None,
            "a middle substring is not a completion"
        );
        let mut bar = AddressBar::new();
        bar.open("");
        bar.insert("htt");
        assert_eq!(
            bar.autocomplete(&history).as_deref(),
            Some("https://example.com/")
        );
        assert!(bar.accept_completion(&history));
        assert_eq!(bar.text(), "https://example.com/");
        assert_eq!(bar.selected_text(), Some("ps://example.com/"));
    }
}
