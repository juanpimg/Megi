//! Bounded deterministic caches for parsed CSS stylesheets and laid-out box trees (#947, #990).

use std::collections::HashMap;

use css::ComputedStyle;
use dom::{Document, NodeId, NodeKind};
use layout::{LayoutBox, Rect, TextMeasurer};

const MAX_LAYOUT_CACHE_ENTRIES: usize = 16;
const MAX_CSS_CACHE_ENTRIES: usize = 64;

/// Hit/miss counters for [`RenderCache`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderCacheStats {
    pub layout_hits: usize,
    pub layout_misses: usize,
    pub css_hits: usize,
    pub css_misses: usize,
}

/// Reusable in-process cache for repeated `render_url_cached` invocations.
#[derive(Clone, Debug, Default)]
pub struct RenderCache {
    layout_entries: Vec<(u64, LayoutBox)>,
    css_entries: Vec<(u64, usize, css::Stylesheet)>,
    stats: RenderCacheStats,
}

impl RenderCache {
    /// Creates an empty [`RenderCache`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns accumulated cache hit/miss counters.
    pub fn stats(&self) -> RenderCacheStats {
        self.stats
    }

    /// Clears all cached entries and resets statistics.
    pub fn clear(&mut self) {
        self.layout_entries.clear();
        self.css_entries.clear();
        self.stats = RenderCacheStats::default();
    }

    /// Parses a stylesheet or returns a cached clone when `(hash, len)` matches.
    pub fn parse_stylesheet(&mut self, css_text: &str) -> css::Stylesheet {
        let key_hash = fnv1a_bytes(css_text.as_bytes());
        let key_len = css_text.len();
        if let Some(pos) = self
            .css_entries
            .iter()
            .position(|(h, len, _)| *h == key_hash && *len == key_len)
        {
            self.stats.css_hits += 1;
            let entry = self.css_entries.remove(pos);
            let sheet = entry.2.clone();
            self.css_entries.push(entry);
            return sheet;
        }
        self.stats.css_misses += 1;
        let sheet = css::parse_stylesheet(css_text);
        if self.css_entries.len() >= MAX_CSS_CACHE_ENTRIES {
            self.css_entries.remove(0);
        }
        self.css_entries.push((key_hash, key_len, sheet.clone()));
        sheet
    }

    /// Computes or reuses a [`LayoutBox`] keyed by DOM + style + viewport + replaced-size fingerprint.
    pub fn get_or_compute_layout(
        &mut self,
        document: &Document,
        styles: &HashMap<NodeId, ComputedStyle>,
        viewport: Rect,
        measurer: &dyn TextMeasurer,
        replaced_sizes: &HashMap<NodeId, (f32, f32)>,
        font_count: usize,
    ) -> Option<(LayoutBox, bool)> {
        let fp = layout_fingerprint(document, styles, viewport, replaced_sizes, font_count);
        if let Some(pos) = self.layout_entries.iter().position(|(k, _)| *k == fp) {
            self.stats.layout_hits += 1;
            let entry = self.layout_entries.remove(pos);
            let cloned = entry.1.clone();
            self.layout_entries.push(entry);
            return Some((cloned, true));
        }
        self.stats.layout_misses += 1;
        let layout_root = layout::layout_document_with_measurer_and_replaced_sizes(
            document,
            styles,
            viewport,
            measurer,
            Some(replaced_sizes),
        )?;
        if self.layout_entries.len() >= MAX_LAYOUT_CACHE_ENTRIES {
            self.layout_entries.remove(0);
        }
        self.layout_entries.push((fp, layout_root.clone()));
        Some((layout_root, false))
    }
}

fn fnv1a_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn mix_u64(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x1000_0000_01b3);
}

fn mix_str(hash: &mut u64, s: &str) {
    mix_u64(hash, s.len() as u64);
    for &b in s.as_bytes() {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
}

/// Computes a deterministic 64-bit fingerprint over the DOM, computed styles, viewport,
/// replaced image intrinsic sizes, and loaded font count.
pub fn layout_fingerprint(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
    replaced_sizes: &HashMap<NodeId, (f32, f32)>,
    font_count: usize,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    mix_u64(&mut hash, u64::from(viewport.x.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.y.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.width.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.height.to_bits()));
    mix_u64(&mut hash, font_count as u64);

    let mut stack = vec![document.root()];
    while let Some(id) = stack.pop() {
        let node = document.node(id);
        match &node.kind {
            NodeKind::Document => mix_u64(&mut hash, 1),
            NodeKind::Doctype { name, .. } => {
                mix_u64(&mut hash, 2);
                mix_str(&mut hash, name);
            }
            NodeKind::Element(el) => {
                mix_u64(&mut hash, 3);
                mix_str(&mut hash, &el.local_name);
                mix_u64(&mut hash, el.attributes.len() as u64);
                for attr in &el.attributes {
                    mix_str(&mut hash, &attr.local_name);
                    mix_str(&mut hash, &attr.value);
                }
                if let Some(style) = styles.get(&id) {
                    let fp = style.fingerprint();
                    mix_u64(
                        &mut hash,
                        if fp != 0 {
                            fp
                        } else {
                            style.compute_fingerprint()
                        },
                    );
                } else {
                    mix_u64(&mut hash, 0);
                }
            }
            NodeKind::Text(text) => {
                mix_u64(&mut hash, 4);
                mix_str(&mut hash, text);
            }
            NodeKind::Comment(c) => {
                mix_u64(&mut hash, 5);
                mix_str(&mut hash, c);
            }
            NodeKind::DocumentFragment => mix_u64(&mut hash, 7),
            NodeKind::ProcessingInstruction { target, data } => {
                mix_u64(&mut hash, 8);
                mix_str(&mut hash, target);
                mix_str(&mut hash, data);
            }
        }
        if let Some((rw, rh)) = replaced_sizes.get(&id) {
            mix_u64(&mut hash, 6);
            mix_u64(&mut hash, u64::from(rw.to_bits()));
            mix_u64(&mut hash, u64::from(rh.to_bits()));
        }
        let children: Vec<NodeId> = document.children(id).collect();
        mix_u64(&mut hash, children.len() as u64);
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    hash
}
