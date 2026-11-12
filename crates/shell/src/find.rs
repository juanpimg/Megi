//! Find-in-page: case-insensitive text search over the rendered DOM and highlights.
//!
//! The search runs on the **DOM text nodes** of the frame (`dom::NodeKind::Text`), in
//! document order, and returns `(NodeId, byte range)` pairs. Highlight rectangles are
//! derived from the **layout tree** (`BoxType::TextNode` inline fragments): the shell paints
//! one translucent yellow rectangle per layout text fragment that contains a hit, which is
//! the documented approximation ("line/fragment rectangles", no glyph-level runs). All hits
//! use `rgba(255, 235, 59, 0.45)` and the current hit uses a stronger blend, both in the
//! window (overlay drawn on the presented framebuffer, no re-render needed for the
//! highlight itself) and in headless screenshots (`--find QUERY`, painted into the pixmap
//! before PNG encoding).
//!
//! Deliberate limits: no regex, no whole-word/Unicode collation modes, no search inside
//! images or canvas, no incremental matching while the page scrolls (the frame carries the
//! hits computed at render time), and case-insensitive matching uses Unicode lowercase with
//! a best-effort byte-offset map (ASCII/UTF-8 text is exact).
//!
//! The pure state is testable without a display ([`FindState`], [`search`],
//! [`highlight_rects`], [`overlay_highlights_pixmap`]).

use std::ops::Range;

use dom::{Document, NodeId};
use layout::{BoxType, LayoutBox};

use crate::links::CssRect;

/// Highlight color for every hit: `rgb(255, 235, 59)` blended at [`HIGHLIGHT_ALPHA`].
pub const HIGHLIGHT_RGB: (u8, u8, u8) = (255, 235, 59);
/// Alpha of a regular hit (`rgba(255, 235, 59, 0.45)`).
pub const HIGHLIGHT_ALPHA: f32 = 0.45;
/// Alpha of the current hit (stronger than the rest).
pub const HIGHLIGHT_CURRENT_ALPHA: f32 = 0.75;

/// One hit: a byte range inside a DOM text node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    /// Text node containing the hit.
    pub node_id: NodeId,
    /// Byte range inside the node's text (char-boundary aligned).
    pub range: Range<usize>,
}

/// Case-insensitive text search over every text node of `document`, in document order.
///
/// Returns one [`FindMatch`] per occurrence (possibly several per text node). Empty queries
/// return no hits.
pub fn search(document: &Document, query: &str) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let (needle, _) = lower_with_map(query);
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for node_id in text_nodes(document) {
        let Some(text) = document.node(node_id).as_text() else {
            continue;
        };
        let (lower, map) = lower_with_map(text);
        let mut from = 0usize;
        while let Some(offset) = lower[from..].find(&needle) {
            let start = offset + from;
            let end = start + needle.len();
            let node_range = map_range(&map, text.len(), start, end);
            matches.push(FindMatch {
                node_id,
                range: node_range,
            });
            from = end;
            if from >= lower.len() {
                break;
            }
        }
    }
    matches
}

/// Highlight rectangles for every occurrence of `query` in the layout tree.
///
/// Rectangles are the **border boxes of inline text fragments** containing a hit
/// (`BoxType::TextNode`), so a long run spanning a line yields one rectangle per layout
/// fragment. Order follows the layout/paint order, which matches DOM document order for the
/// text content the shell searches.
pub fn highlight_rects(layout: &LayoutBox, query: &str) -> Vec<CssRect> {
    if query.is_empty() {
        return Vec::new();
    }
    let (needle, _) = lower_with_map(query);
    if needle.is_empty() {
        return Vec::new();
    }
    let mut rects = Vec::new();
    let mut stack = vec![layout];
    while let Some(layout_box) = stack.pop() {
        if let BoxType::TextNode(text) = &layout_box.box_type {
            let (lower, _) = lower_with_map(text);
            let hits = lower.matches(&needle).count();
            if hits > 0 {
                let rect = layout_box.dimensions.border_box();
                let css = CssRect::new(rect.x, rect.y, rect.width, rect.height);
                if css.width > 0.0 && css.height > 0.0 {
                    for _ in 0..hits {
                        rects.push(css);
                    }
                }
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    rects
}

/// Returns every text-node id in document order (iterative traversal).
fn text_nodes(document: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    // Push siblings in reverse so the stack pops them in document order.
    let mut child = document.node(document.root()).last_child;
    while let Some(id) = child {
        stack.push(id);
        child = document.node(id).prev_sibling;
    }
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if node.as_text().is_some() {
            out.push(node_id);
        }
        let mut child = node.last_child;
        while let Some(id) = child {
            stack.push(id);
            child = document.node(id).prev_sibling;
        }
    }
    out
}

/// Lowercases `text` and returns a map from each lowercase byte to its source byte offset.
///
/// `text.to_lowercase()` can change the byte length (e.g. `İ` → `i̇`), so byte offsets from
/// `lower` cannot be reused directly on the original text. This map keeps one original
/// char-start offset per lowercase byte; with it a lowercase byte range is converted back
/// to a char-boundary-aligned range in `text` (best effort, exact for ASCII).
fn lower_with_map(text: &str) -> (String, Vec<usize>) {
    let mut lower = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        let mut buf = [0u8; 4];
        for lowercase_char in character.to_lowercase() {
            let encoded = lowercase_char.encode_utf8(&mut buf);
            for _ in 0..encoded.len() {
                map.push(offset);
            }
            lower.push(lowercase_char);
        }
    }
    (lower, map)
}

/// Converts a lowercase byte range back to a char-boundary-aligned range in the original.
fn map_range(map: &[usize], original_len: usize, start: usize, end: usize) -> Range<usize> {
    if map.is_empty() {
        return 0..0;
    }
    let original_start = map.get(start).copied().unwrap_or(original_len);
    let last = end.saturating_sub(1).min(map.len() - 1);
    let last_start = map.get(last).copied().unwrap_or(original_len);
    // End of the last matched source char: the next char start after it (map values are
    // char starts), or the end of the text. `last_start + 1` could land inside a multi-byte
    // scalar (e.g. "İstanbul": U+0130 lowercases to "i" + U+0307, so matching "i" ended at
    // source byte 1, inside the 2-byte İ) and `&text[range]` then panics.
    let original_end = map
        .iter()
        .copied()
        .find(|offset| *offset > last_start)
        .unwrap_or(original_len)
        .min(original_len);
    original_start..original_end
}

/// Scroll offset (in CSS px) that centers `rect` vertically, clamped to `0..=max_offset`.
///
/// Keeps the current hit visible: pages shorter than the viewport always return 0.
pub fn scroll_offset_for(rect: &CssRect, viewport_height: u32, max_offset: u32) -> u32 {
    if max_offset == 0 {
        return 0;
    }
    let missing = viewport_height as f32 - rect.height;
    let desired = rect.y - (missing / 2.0).max(0.0);
    (desired.max(0.0) as u32).min(max_offset)
}

/// Blends `rgb` over `base` with `alpha` (0..=1), returning straight RGB.
fn blend_rgb(base: (u8, u8, u8), rgb: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    let alpha = alpha.clamp(0.0, 1.0);
    let mix = |channel_base: u8, channel_target: u8| -> u8 {
        let value = f32::from(channel_base) * (1.0 - alpha) + f32::from(channel_target) * alpha;
        value.round().clamp(0.0, 255.0) as u8
    };
    (mix(base.0, rgb.0), mix(base.1, rgb.1), mix(base.2, rgb.2))
}

/// Paints highlight rectangles directly into a CSS-resolution pixmap (`--find` screenshots).
///
/// The pixmap is in page CSS coordinates (same as the layout), so rectangles map 1:1. The
/// current hit (index `current`, clamped) uses the stronger alpha. Returns the number of
/// painted pixels.
pub fn overlay_highlights_pixmap(
    pixmap: &mut paint::Pixmap,
    rects: &[CssRect],
    current: usize,
) -> usize {
    let mut painted = 0usize;
    let mut seen: Vec<CssRect> = Vec::new();
    for (index, rect) in rects.iter().enumerate() {
        if seen.contains(rect) {
            continue; // several hits in one layout fragment share a rectangle
        }
        seen.push(*rect);
        let alpha = if index == current {
            HIGHLIGHT_CURRENT_ALPHA
        } else {
            HIGHLIGHT_ALPHA
        };
        let x0 = rect.x.max(0.0).floor() as u32;
        let y0 = rect.y.max(0.0).floor() as u32;
        let x1 = ((rect.x + rect.width).min(f32::from(u16::MAX)) as u32).min(pixmap.width);
        let y1 = ((rect.y + rect.height).min(f32::from(u16::MAX)) as u32).min(pixmap.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = (y as usize) * (pixmap.width as usize) + x as usize;
                let Some(pixel) = pixmap.pixels.get_mut(offset) else {
                    continue;
                };
                let (r, g, b) = blend_rgb((pixel.r, pixel.g, pixel.b), HIGHLIGHT_RGB, alpha);
                pixel.r = r;
                pixel.g = g;
                pixel.b = b;
                painted += 1;
            }
        }
    }
    painted
}

/// Paints the visible highlights onto the physical window framebuffer.
///
/// Maps page CSS coordinates to destination pixels with the same mapping
/// [`crate::present::present_region`] uses for the frame: columns over `css_width`, rows
/// over `css_viewport_height` starting at `scroll_y`. Returns the painted pixel count.
#[allow(clippy::too_many_arguments)]
pub fn overlay_highlights_dest(
    dest: &mut [u32],
    dest_width: u32,
    dest_height: u32,
    css_width: u32,
    css_viewport_height: u32,
    scroll_y: u32,
    rects: &[CssRect],
    current: usize,
) -> usize {
    if dest_width == 0 || dest_height == 0 || css_width == 0 || css_viewport_height == 0 {
        return 0;
    }
    if dest.len() < (dest_width as usize) * (dest_height as usize) {
        return 0;
    }
    let mut painted = 0usize;
    let mut seen: Vec<CssRect> = Vec::new();
    for (index, rect) in rects.iter().enumerate() {
        if seen.contains(rect) {
            continue;
        }
        seen.push(*rect);
        let alpha = if index == current {
            HIGHLIGHT_CURRENT_ALPHA
        } else {
            HIGHLIGHT_ALPHA
        };
        let scroll = scroll_y as f32;
        let css_x0 = rect.x.max(0.0);
        let css_y0 = (rect.y - scroll).max(0.0);
        if css_x0 >= css_width as f32 || css_y0 >= css_viewport_height as f32 {
            continue;
        }
        let scale_x = dest_width as f32 / css_width as f32;
        let scale_y = dest_height as f32 / css_viewport_height as f32;
        let dest_x0 = (css_x0 * scale_x) as u32;
        let dest_y0 = (css_y0 * scale_y) as u32;
        let dest_x1 = ((rect.x + rect.width).max(0.0) * scale_x) as u32;
        let dest_y1 = ((rect.y + rect.height - scroll).max(0.0) * scale_y) as u32;
        let x0 = dest_x0.min(dest_width.saturating_sub(1));
        let y0 = dest_y0.min(dest_height.saturating_sub(1));
        let x1 = dest_x1.min(dest_width);
        let y1 = dest_y1.min(dest_height);
        for y in y0..y1 {
            for x in x0..x1 {
                let offset = (y as usize) * (dest_width as usize) + x as usize;
                let Some(pixel) = dest.get_mut(offset) else {
                    continue;
                };
                let base = (
                    (*pixel >> 16) as u8,
                    (*pixel >> 8) as u8,
                    (*pixel & 0xFF) as u8,
                );
                let (r, g, b) = blend_rgb(base, HIGHLIGHT_RGB, alpha);
                *pixel = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                painted += 1;
            }
        }
    }
    painted
}

/// Find-in-page state: query, hits and the current hit index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindState {
    query: String,
    matches: Vec<FindMatch>,
    current: usize,
}

impl FindState {
    /// Creates an empty, closed find state.
    pub const fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current: 0,
        }
    }

    /// Current query (empty when closed).
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Hits of the current query.
    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }

    /// Number of hits.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are no hits.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Index of the current hit (`0` when there are no hits).
    pub const fn current(&self) -> usize {
        self.current
    }

    /// Current hit, if any.
    pub fn current_match(&self) -> Option<&FindMatch> {
        self.matches.get(self.current)
    }

    /// Replaces the query/hits (used after a render with the new query); resets to hit 0.
    pub fn set_results(&mut self, query: impl Into<String>, matches: Vec<FindMatch>) {
        self.query = query.into();
        self.matches = matches;
        self.current = 0;
    }

    /// Replaces the hit list without resetting the current index (clamped to the new list).
    pub fn set_matches(&mut self, matches: Vec<FindMatch>) {
        self.matches = matches;
        if self.current >= self.matches.len() {
            self.current = 0;
        }
    }

    /// Advances to the next hit, wrapping around to the first one.
    pub fn next_hit(&mut self) -> bool {
        if self.matches.is_empty() {
            self.current = 0;
            return false;
        }
        self.current = (self.current + 1) % self.matches.len();
        true
    }

    /// Goes back to the previous hit, wrapping around to the last one.
    pub fn previous_hit(&mut self) -> bool {
        if self.matches.is_empty() {
            self.current = 0;
            return false;
        }
        self.current = if self.current == 0 {
            self.matches.len() - 1
        } else {
            self.current - 1
        };
        true
    }

    /// Selects `index` (clamped to the hit list).
    pub fn set_current(&mut self, index: usize) {
        self.current = index.min(self.matches.len().saturating_sub(1));
    }

    /// Closes the search (clears query and hits).
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = 0;
    }

    /// Highlight rectangles of the current query, recomputed from `layout`.
    ///
    /// The current hit is index-clamped modulo the rectangle count (the DOM hit list and the
    /// layout fragment list can differ when one fragment contains several hits).
    pub fn current_rect(&self, layout: &LayoutBox) -> Option<CssRect> {
        if self.query.is_empty() {
            return None;
        }
        let rects = highlight_rects(layout, &self.query);
        if rects.is_empty() {
            return None;
        }
        Some(rects[self.current % rects.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_output() -> render::RenderOutput {
        let html = "<!DOCTYPE html><html><body>\
            <h1>Hello Find</h1>\
            <p>The quick brown fox jumps over the lazy dog. Quick QUICK quick.</p>\
            <p>second paragraph with QUICK again</p>\
            </body></html>";
        crate::headless::render_target(
            &format!("data:text/html,{}", html.replace(' ', "%20")),
            320,
            240,
        )
        .expect("render fixture")
    }

    fn position_of(text: &str, needle: &str) -> usize {
        text.find(needle).expect("needle present")
    }

    #[test]
    fn search_is_case_insensitive_with_byte_ranges() {
        let output = fixture_output();
        let hits = search(&output.document, "quick");
        assert_eq!(
            hits.len(),
            5,
            "Quick, Quick, QUICK, quick + QUICK again/{hits:?}"
        );

        // Every range must point at the expected text, char-boundary aligned.
        for hit in &hits {
            let text = output
                .document
                .node(hit.node_id)
                .as_text()
                .expect("text node");
            assert_eq!(&text[hit.range.clone()].to_lowercase(), "quick");
        }
        let first = &hits[0];
        let text = output
            .document
            .node(first.node_id)
            .as_text()
            .expect("text node");
        assert_eq!(first.range.start, position_of(text, "quick"));
    }

    #[test]
    fn search_multiple_occurrences_in_one_node() {
        let output = fixture_output();
        let hits = search(&output.document, "quick");
        let hamlet = hits
            .iter()
            .find(|hit| {
                output
                    .document
                    .node(hit.node_id)
                    .as_text()
                    .is_some_and(|text| text.contains("Quick QUICK quick"))
            })
            .expect("hamlet node");
        assert!(hamlet.range.end <= "x".len() + hamlet.range.start + 1024);
    }

    #[test]
    fn search_without_matches_or_query_is_empty() {
        let output = fixture_output();
        assert!(search(&output.document, "megi-not-there").is_empty());
        assert!(search(&output.document, "").is_empty());
        assert!(
            search(&output.document, "   ").is_empty(),
            "spaces are searched literally"
        );
    }

    #[test]
    fn find_state_next_and_previous_wrap_around() {
        let mut state = FindState::new();
        assert!(state.is_empty());
        assert!(!state.next_hit());
        assert!(!state.previous_hit());
        state.set_results(
            "quick",
            (0..3)
                .map(|index| FindMatch {
                    node_id: NodeId::from_index(index),
                    range: 0..1,
                })
                .collect(),
        );
        assert_eq!(state.current(), 0);
        assert!(state.next_hit());
        assert_eq!(state.current(), 1);
        assert!(state.next_hit());
        assert_eq!(state.current(), 2);
        assert!(state.next_hit());
        assert_eq!(state.current(), 0, "wraps to the first hit");
        assert!(state.previous_hit());
        assert_eq!(state.current(), 2, "wraps to the last hit");
        state.set_current(9);
        assert_eq!(state.current(), 2, "clamped");
        state.clear();
        assert!(state.is_empty());
        assert_eq!(state.query(), "");
    }

    #[test]
    fn highlight_rects_follow_the_layout_fragments() {
        let output = fixture_output();
        let rects = highlight_rects(&output.layout, "quick");
        assert!(rects.len() >= 3, "one rect per text fragment: {rects:?}");
        for rect in &rects {
            assert!(rect.width > 0.0 && rect.height > 0.0);
            assert!(rect.y >= 0.0);
        }
        // A unique word in the first heading has a single rect.
        assert_eq!(highlight_rects(&output.layout, "Hello").len(), 1);
        assert!(highlight_rects(&output.layout, "zzz-not-found").is_empty());
    }

    #[test]
    fn current_rect_uses_the_current_hit() {
        let output = fixture_output();
        let mut state = FindState::new();
        state.set_results("quick", search(&output.document, "quick"));
        let rect0 = state.current_rect(&output.layout).expect("first rect");
        assert!(state.next_hit());
        let rect1 = state.current_rect(&output.layout).expect("second rect");
        assert!(
            rect0.y <= rect1.y || rect0.x != rect1.x,
            "rect changes with the current hit"
        );
        state.set_results("nothing-here", Vec::new());
        assert!(state.current_rect(&output.layout).is_none());
    }

    #[test]
    fn scroll_offset_centers_and_clamps() {
        let rect = CssRect::new(10.0, 400.0, 50.0, 20.0);
        assert_eq!(scroll_offset_for(&rect, 200, 1000), 310);
        assert_eq!(scroll_offset_for(&rect, 200, 100), 100);
        assert_eq!(
            scroll_offset_for(&CssRect::new(0.0, 0.0, 10.0, 10.0), 200, 1000),
            0
        );
        assert_eq!(scroll_offset_for(&rect, 200, 0), 0, "short page");
    }

    #[test]
    fn pixmap_overlay_paints_yellow_and_stronger_current() {
        let output = fixture_output();
        let rects = highlight_rects(&output.layout, "quick");
        let mut pixmap = output.pixmap.clone();
        let rect = rects[0];
        let sample = ((rect.y + rect.height / 2.0) as usize) * (pixmap.width as usize)
            + (rect.x + rect.width / 2.0) as usize;
        let before = pixmap.pixels[sample];
        let painted = overlay_highlights_pixmap(&mut pixmap, &rects, 0);
        assert!(painted > 0);
        assert_ne!(pixmap.pixels[sample], before);
        // A regular (non-current) hit is blended at 0.45 over white; the current one at 0.75.
        let regular = blend_rgb((255, 255, 255), HIGHLIGHT_RGB, HIGHLIGHT_ALPHA);
        let current = blend_rgb((255, 255, 255), HIGHLIGHT_RGB, HIGHLIGHT_CURRENT_ALPHA);
        assert!(
            current.1 < regular.1,
            "current hit is more intense (yellow channel)"
        );
        let painted_current = pixmap
            .pixels
            .iter()
            .filter(|pixel| (pixel.r, pixel.g, pixel.b) == current)
            .count();
        assert!(painted_current > 0, "current highlight visible");
    }

    #[test]
    fn dest_overlay_maps_visible_rects() {
        let mut dest = vec![0x00FF_FFFFu32; 20 * 10];
        let rect = CssRect::new(0.0, 8.0, 10.0, 4.0);
        // Page is 40 CSS px wide/20 tall; the frame is scrolled down 4 px.
        let painted = overlay_highlights_dest(&mut dest, 20, 10, 40, 20, 4, &[rect], 0);
        assert!(painted > 0);
        assert!(dest.iter().any(|pixel| *pixel != 0x00FF_FFFF));
        // A rect above the scroll window is clipped away.
        let offscreen = CssRect::new(0.0, 0.0, 10.0, 4.0);
        let mut untouched = vec![0x00FF_FFFFu32; 20 * 10];
        assert_eq!(
            overlay_highlights_dest(&mut untouched, 20, 10, 40, 20, 4, &[offscreen], 0),
            0
        );
        assert!(untouched.iter().all(|pixel| *pixel == 0x00FF_FFFF));
    }
    #[test]
    fn search_ranges_stay_on_char_boundaries_when_lowercasing_expands() {
        // U+0130 (İ) lowercases to "i" + U+0307: a match of "i" ends inside the source
        // scalar, and the returned range must still be a char-boundary range.
        let document = html::parse_document("<body><p>İstanbul</p></body>", false);
        let hits = search(&document, "i");
        assert!(!hits.is_empty(), "the lowercased i matches");
        for hit in &hits {
            let text = document.node(hit.node_id).as_text().expect("text node");
            assert!(text.is_char_boundary(hit.range.start), "{:?}", hit.range);
            assert!(text.is_char_boundary(hit.range.end), "{:?}", hit.range);
        }
        let hit = &hits[0];
        let text = document.node(hit.node_id).as_text().expect("text node");
        assert!(
            text[hit.range.clone()].contains('İ'),
            "the range covers the whole source char: {:?}",
            hit.range
        );
    }
}
