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
//! Deliberate limits: no autocomplete, no visual history dropdown, no icons/favicons,
//! no native menus, ASCII text rendered with the paint crate's built-in 8×8 bitmap font
//! (uppercase glyphs used for lowercase input), and the address bar edits the URL only
//! (search terms are treated as URLs). The tab strip shows a compact title per tab with
//! `+`/`×` affordances and a `+N` overflow marker; there is no drag/reorder.

use layout::Rect;
use paint::{Color, Pixmap};

use crate::forms::{SelectOption, TextEdit};
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

/// Address bar edition state (URL text + a simple caret; no selection ranges).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddressBar {
    edit: TextEdit,
    editing: bool,
}

impl AddressBar {
    /// Creates a closed address bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens edition pre-filled with `url` (caret at the end).
    pub fn open(&mut self, url: &str) {
        self.edit = TextEdit::new(url);
        self.editing = true;
    }

    /// Closes edition (cancelling any unsubmitted text).
    pub fn cancel(&mut self) {
        self.editing = false;
        self.edit = TextEdit::new("");
    }

    /// Whether the address bar is being edited.
    pub const fn is_editing(&self) -> bool {
        self.editing
    }

    /// Current text.
    pub fn text(&self) -> &str {
        self.edit.value()
    }

    /// Caret byte offset.
    pub const fn caret(&self) -> usize {
        self.edit.caret()
    }

    /// Inserts text at the caret.
    pub fn insert(&mut self, text: &str) -> bool {
        self.edit.insert(text)
    }

    /// Deletes the character before the caret.
    pub fn backspace(&mut self) -> bool {
        self.edit.backspace()
    }

    /// Moves the caret.
    pub fn move_caret(&mut self, right: bool) -> bool {
        self.edit.move_caret(right)
    }

    /// Closes edition and returns whether it was open.
    pub fn finish(&mut self) -> bool {
        let was = self.editing;
        self.editing = false;
        was
    }
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
    }
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
    /// Total number of options.
    pub total: usize,
    /// Whether the list opens above the control.
    pub above: bool,
}

impl DropdownList {
    /// Option index under a chrome point (accounting for the scroll window).
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
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

    /// Moves the active option up/down, scrolling the window when needed.
    pub fn move_active(&mut self, down: bool) -> bool {
        if self.options.is_empty() {
            return false;
        }
        let current = self.active.unwrap_or(0);
        let next = if down {
            (current + 1).min(self.options.len() - 1)
        } else {
            current.saturating_sub(1)
        };
        self.active = Some(next);
        if next < self.scroll {
            self.scroll = next;
        } else if next >= self.scroll + DROPDOWN_MAX_VISIBLE {
            self.scroll = next + 1 - DROPDOWN_MAX_VISIBLE;
        }
        next != current
    }

    /// Active option index.
    pub const fn active(&self) -> Option<usize> {
        self.active
    }

    /// Whether the dropdown is non-empty.
    pub fn is_open(&self) -> bool {
        !self.options.is_empty()
    }
}

/// All data the chrome painter needs (plain references, no shell state).
#[derive(Clone, Copy, Debug)]
pub struct ChromeView<'a> {
    /// Tab titles, in order.
    pub tabs: &'a [String],
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
        draw_text(
            pixmap,
            &tab.title,
            tab.rect.x + 6.0,
            tab.rect.y + 8.0,
            scale,
            11.0,
            COLOR_TEXT,
        );
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
    let list = dropdown_layout(
        &dropdown.control_rect,
        dropdown.options.len(),
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
        let option_index = list.first + row;
        let Some(option) = dropdown.options.get(option_index) else {
            break;
        };
        let background = if dropdown.active == Some(option_index) {
            COLOR_DROPDOWN_ACTIVE
        } else if option.selected {
            COLOR_DROPDOWN_SELECTED
        } else {
            COLOR_DROPDOWN_BG
        };
        pixmap.fill_rect(scaled(*rect, scale), background);
        draw_text(
            pixmap,
            &option.label,
            rect.x + 5.0,
            rect.y + 5.0,
            scale,
            11.0,
            COLOR_ADDRESS_TEXT,
        );
    }
    if list.first > 0
        && let Some(first) = list.rows.first()
    {
        pixmap.fill_rect(
            Rect::new(
                first.x * scale,
                (first.y - 2.0) * scale,
                8.0 * scale,
                2.0 * scale,
            ),
            COLOR_SCROLL,
        );
    }
    if list.first + list.visible < list.total
        && let Some(last) = list.rows.last()
    {
        pixmap.fill_rect(
            Rect::new(
                last.x * scale,
                (last.y + last.height) * scale,
                8.0 * scale,
                2.0 * scale,
            ),
            COLOR_SCROLL,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn view<'a>(
        address: &'a AddressBar,
        dropdown: Option<&'a DropdownState>,
        tabs: &'a [String],
    ) -> ChromeView<'a> {
        ChromeView {
            tabs,
            active_tab: 0,
            address,
            fallback_url: "https://example.com/",
            dropdown,
            scroll_y: 0,
            css_viewport_height: 400,
            css_width: 640.0,
        }
    }

    fn buffer(width: u32, height: u32) -> Vec<u32> {
        vec![0x00FF_FFFF; (width * height) as usize]
    }

    #[test]
    fn address_bar_edits_and_keeps_the_caret() {
        let mut bar = AddressBar::new();
        assert!(!bar.is_editing());
        bar.open("https://example.com/a");
        assert!(bar.is_editing());
        assert_eq!(bar.caret(), "https://example.com/a".len());
        assert!(bar.backspace());
        assert_eq!(bar.text(), "https://example.com/");
        bar.move_caret(true); // no-op at the end
        assert!(bar.insert("X"));
        assert_eq!(bar.text(), "https://example.com/X");
        assert!(bar.finish());
        assert!(!bar.is_editing());
        bar.open("x");
        bar.cancel();
        assert!(!bar.is_editing());
        assert_eq!(bar.text(), "");
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
        assert_eq!(list.hit(row.x + 2.0, row.y + 2.0), Some(1));
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
}
