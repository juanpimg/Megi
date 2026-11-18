//! Tab strip and its hidden-tab overflow menu: geometry, hit-testing and painting.

use megi_webview::layout::Rect;
use megi_webview::paint::Pixmap;

use super::drawing::*;
use super::*;

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

pub(crate) fn paint_tab_strip(pixmap: &mut Pixmap, scale: f32, view: &ChromeView<'_>, width: f32) {
    pixmap.fill_rect(
        Rect::new(0.0, 0.0, width * scale, TAB_STRIP_HEIGHT * scale),
        COLOR_TAB_STRIP,
    );
    let layout = layout_tabs(view.tabs, width, view.active_tab);
    for tab in &layout.tabs {
        let active = tab.index == view.active_tab;
        let color = if active { COLOR_TAB_ACTIVE } else { COLOR_TAB };
        pixmap.fill_rounded_rect(
            scaled(tab.rect, scale),
            top_radii(TAB_RADIUS * scale),
            color,
        );
        if !active && tab.index > 0 {
            pixmap.fill_rect(
                Rect::new(
                    tab.rect.x * scale,
                    6.0 * scale,
                    scale,
                    (TAB_STRIP_HEIGHT - 6.0) * scale,
                ),
                COLOR_TAB_SEPARATOR,
            );
        }
        if active {
            // The accent underline is the tab's identity marker: it reads at a glance
            // which page is visible without relying on a lighter background alone.
            pixmap.fill_rect(
                Rect::new(
                    (tab.rect.x + 2.0) * scale,
                    (TAB_STRIP_HEIGHT - 2.0) * scale,
                    (tab.rect.width - 4.0).max(0.0) * scale,
                    2.0 * scale,
                ),
                ACCENT,
            );
        }
        let mut title_x = tab.rect.x + 6.0;
        if let Some(Some(icon)) = view.favicons.get(tab.index) {
            title_x = tab.rect.x + 23.0;
            paint_favicon(pixmap, icon, tab.rect.x + 3.0, tab.rect.y + 4.0, scale);
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
        fill_round(pixmap, close, scale, 3.0, COLOR_ADDRESS_BG);
        draw_text(
            pixmap,
            "x",
            close.x + 3.0,
            close.y + 2.0,
            scale,
            11.0,
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

pub(crate) fn paint_overflow_menu(
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
    paint_panel(pixmap, layout.rect, scale, MENU_RADIUS, COLOR_MENU_BG);
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
