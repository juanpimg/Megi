//! Bounded deterministic caches for parsed CSS stylesheets and laid-out box trees (#947, #990).

use std::collections::HashMap;

use css::{ComputedStyle, PseudoElement, PseudoStyles};
use megi_core::{Document, NodeId, NodeKind};
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
    /// `(FNV prefilter, exact source text, parsed sheet)`. The source text is kept so a
    /// hash collision cannot return a different stylesheet (#1223).
    css_entries: Vec<(u64, String, css::Stylesheet)>,
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

    /// Parses a stylesheet or returns a cached clone when the exact source text matches.
    pub fn parse_stylesheet(&mut self, css_text: &str) -> css::Stylesheet {
        let key_hash = fnv1a_bytes(css_text.as_bytes());
        if let Some(pos) = self.css_entries.iter().position(|(h, source, _)| {
            *h == key_hash && source.len() == css_text.len() && source == css_text
        }) {
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
        self.css_entries
            .push((key_hash, css_text.to_owned(), sheet.clone()));
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
        font_digest: u64,
    ) -> Option<(LayoutBox, bool)> {
        let empty = PseudoStyles::new();
        self.get_or_compute_layout_with_pseudos(
            document,
            styles,
            &empty,
            viewport,
            measurer,
            replaced_sizes,
            font_digest,
        )
    }

    /// Computes or reuses a [`LayoutBox`] keyed by DOM + styles + pseudo-element styles +
    /// viewport + replaced-size fingerprint.
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_compute_layout_with_pseudos(
        &mut self,
        document: &Document,
        styles: &HashMap<NodeId, ComputedStyle>,
        pseudo_styles: &PseudoStyles,
        viewport: Rect,
        measurer: &dyn TextMeasurer,
        replaced_sizes: &HashMap<NodeId, (f32, f32)>,
        font_digest: u64,
    ) -> Option<(LayoutBox, bool)> {
        let fp = layout_fingerprint_with_pseudos(
            document,
            styles,
            pseudo_styles,
            viewport,
            replaced_sizes,
            font_digest,
        );
        if let Some(pos) = self.layout_entries.iter().position(|(k, _)| *k == fp) {
            self.stats.layout_hits += 1;
            let entry = self.layout_entries.remove(pos);
            let cloned = entry.1.clone();
            self.layout_entries.push(entry);
            return Some((cloned, true));
        }
        self.stats.layout_misses += 1;
        let layout_root = layout::layout_document_with_measurer_and_pseudos(
            document,
            styles,
            pseudo_styles,
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

fn mix_style_fp(hash: &mut u64, style: &ComputedStyle) {
    let fp = style.fingerprint();
    mix_u64(
        hash,
        if fp != 0 {
            fp
        } else {
            style.compute_fingerprint()
        },
    );
}

/// Computes a deterministic 64-bit fingerprint over the DOM, computed styles, viewport,
/// replaced image intrinsic sizes, and the loaded font identity digest (#1218).
pub fn layout_fingerprint(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    viewport: Rect,
    replaced_sizes: &HashMap<NodeId, (f32, f32)>,
    font_digest: u64,
) -> u64 {
    let empty = PseudoStyles::new();
    layout_fingerprint_with_pseudos(
        document,
        styles,
        &empty,
        viewport,
        replaced_sizes,
        font_digest,
    )
}

/// Computes a deterministic 64-bit fingerprint including `::before` / `::after` styles.
pub fn layout_fingerprint_with_pseudos(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudo_styles: &PseudoStyles,
    viewport: Rect,
    replaced_sizes: &HashMap<NodeId, (f32, f32)>,
    font_digest: u64,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    mix_u64(&mut hash, u64::from(viewport.x.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.y.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.width.to_bits()));
    mix_u64(&mut hash, u64::from(viewport.height.to_bits()));
    mix_u64(&mut hash, font_digest);

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
                    mix_style_fp(&mut hash, style);
                } else {
                    mix_u64(&mut hash, 0);
                }
                if !pseudo_styles.is_empty() {
                    if let Some(before) = pseudo_styles.get(&(id, PseudoElement::Before)) {
                        mix_u64(&mut hash, 0xbef0);
                        mix_style_fp(&mut hash, before);
                    }
                    if let Some(after) = pseudo_styles.get(&(id, PseudoElement::After)) {
                        mix_u64(&mut hash, 0xaf7e);
                        mix_style_fp(&mut hash, after);
                    }
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

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn layout_fingerprint_includes_font_identity() {
        let document = megi_core::Document::new();
        let styles = HashMap::new();
        let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
        let sizes = HashMap::new();
        let a = layout_fingerprint(&document, &styles, viewport, &sizes, 1);
        let b = layout_fingerprint(&document, &styles, viewport, &sizes, 2);
        assert_ne!(
            a, b,
            "different font identities must change the layout key (#1218)"
        );
    }

    #[test]
    fn css_cache_compares_exact_source_on_hash_collision() {
        let a = "a{color:red}";
        let b = "a{color:tan}";
        assert_eq!(a.len(), b.len());
        let sheet_a = css::parse_stylesheet(a);

        let mut cache = RenderCache::new();
        // Inject an entry whose FNV key matches `b` but whose exact source is `a`:
        // a `(hash, len)`-only cache would wrongly return sheet_a on this lookup (#1223).
        cache
            .css_entries
            .push((fnv1a_bytes(b.as_bytes()), a.to_string(), sheet_a.clone()));
        let sheet_b = cache.parse_stylesheet(b);

        assert_ne!(
            sheet_b, sheet_a,
            "a hash collision must not return another sheet"
        );
        assert_eq!(
            cache.stats(),
            RenderCacheStats {
                css_hits: 0,
                css_misses: 1,
                ..Default::default()
            }
        );
    }
}
