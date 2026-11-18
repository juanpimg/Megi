use super::*;

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

/// Maximum matches/rects returned by [`search`]/[`highlight_rects`].
///
/// A one-character query over a large page can match millions of times; the overlay only
/// needs the visible highlight count, so the scan stops at this documented cap.
pub const MAX_FIND_MATCHES: usize = 10_000;

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
            if matches.len() >= MAX_FIND_MATCHES {
                return matches;
            }
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
                    rects.push(css);
                    if rects.len() >= MAX_FIND_MATCHES {
                        return rects;
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
pub(crate) fn text_nodes(document: &Document) -> Vec<NodeId> {
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
pub(crate) fn lower_with_map(text: &str) -> (String, Vec<usize>) {
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
pub(crate) fn map_range(
    map: &[usize],
    original_len: usize,
    start: usize,
    end: usize,
) -> Range<usize> {
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
        .skip(last + 1)
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
pub(crate) fn blend_rgb(base: (u8, u8, u8), rgb: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    let alpha = alpha.clamp(0.0, 1.0);
    let mix = |channel_base: u8, channel_target: u8| -> u8 {
        let value = f32::from(channel_base) * (1.0 - alpha) + f32::from(channel_target) * alpha;
        value.round().clamp(0.0, 255.0) as u8
    };
    (mix(base.0, rgb.0), mix(base.1, rgb.1), mix(base.2, rgb.2))
}

/// Dedupe key for a highlight rectangle (bit patterns, with ±0.0 normalized).
pub(crate) fn rect_key(rect: &CssRect) -> (u32, u32, u32, u32) {
    fn bits(value: f32) -> u32 {
        if value == 0.0 { 0 } else { value.to_bits() }
    }
    (
        bits(rect.x),
        bits(rect.y),
        bits(rect.width),
        bits(rect.height),
    )
}
