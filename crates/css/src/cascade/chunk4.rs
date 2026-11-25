//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

mod document;
#[allow(unused_imports)]
pub(crate) use document::resolve_document_styles_from_collection_inner;

/// Applies embedded CSS animations to already-resolved computed styles at `time_ms`.
///
/// The base styles are used as the underlying values; animated styles are not
/// re-inherited into descendants in this pass (animating an inherited property such as
/// `font-size` affects the element's own box but not its children's inherited value).
/// Callers that need live animation advance `time_ms` and re-run the pass.
pub fn apply_animations_to_styles(
    doc: &Document,
    styles: &mut HashMap<NodeId, crate::properties::ComputedStyle>,
    collection: &StyleCollection,
    time_ms: f32,
) {
    if collection.keyframes.is_empty() || !time_ms.is_finite() {
        return;
    }
    let mut animated: Vec<(NodeId, crate::properties::ComputedStyle)> = Vec::new();
    for node_id in doc.descendants(doc.root()) {
        let Some(base) = styles.get(&node_id) else {
            continue;
        };
        if base.animations.is_empty() {
            continue;
        }
        let style = crate::animation_runtime::apply_animations(
            base,
            &base.animations,
            &collection.keyframes,
            time_ms,
        );
        animated.push((node_id, style));
    }
    for (node_id, style) in animated {
        styles.insert(node_id, style);
    }
}

/// Applies embedded CSS animations to already-resolved pseudo-element styles at `time_ms`.
pub fn apply_animations_to_pseudo_styles(
    pseudo_styles: &mut PseudoStyles,
    collection: &StyleCollection,
    time_ms: f32,
) {
    if collection.keyframes.is_empty() || !time_ms.is_finite() || pseudo_styles.is_empty() {
        return;
    }
    for base in pseudo_styles.values_mut() {
        if base.animations.is_empty() {
            continue;
        }
        *base = crate::animation_runtime::apply_animations(
            base,
            &base.animations,
            &collection.keyframes,
            time_ms,
        );
    }
}

/// Resolves computed styles for all elements in the DOM document.
///
/// Pre-loads the default WHATWG HTML User-Agent stylesheet, then compiles and cascades
/// any author CSS supplied (e.g. from `<style>` tags or external stylesheets). `@media`
/// rules are evaluated for the `screen` medium with an unknown viewport, so width/height
/// features never match; use [`resolve_document_styles_with_viewport`] to provide one.
pub fn resolve_document_styles(
    doc: &Document,
    author_css: Option<&str>,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::default())
}

/// Resolves computed styles with `@media` width/height features evaluated for a viewport.
pub fn resolve_document_styles_with_viewport(
    doc: &Document,
    author_css: Option<&str>,
    width: f32,
    height: f32,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::with_viewport(width, height))
}

/// Hashes an element's winning declarations (and its parent identity) into a 128-bit
/// style-cache key.
///
/// The parent key is mixed with **all 128 bits** preserved: folding its halves with XOR
/// (and deriving the high half from the low one) collapsed the effective entropy to 64
/// bits, so two distinct parents whose halves XOR-ed to the same value aliased to one
/// cached `ComputedStyle`. That leaked a hidden ancestor's inherited `visibility` into
/// unrelated visible subtrees (e.g. a real-world article page rendering blank).
///
/// Declarations are hashed by name and raw value rather than by pointer. Pointer hashing
/// was cheaper but only identifies values when the address is stable and unique; hashing
/// the actual strings is collision-safe and keeps the key independent of allocation layout.
/// Map iteration order is unspecified, so per-declaration 128-bit hashes are XOR-ed together
/// (order-independent) and mixed again into the parent seed.
pub(crate) fn style_cache_key(
    parent_key: u128,
    cascaded: &HashMap<String, &Declaration>,
    max_width_from_ua: bool,
) -> u128 {
    fn mix_u64(state: &mut u128, value: u64) {
        *state ^= u128::from(value.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        *state = state
            .rotate_left(31)
            .wrapping_mul(u128::from(0x100_0000_01b3u64));
    }

    fn hash_str(state: &mut u128, text: &str) {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for byte in text.as_bytes() {
            h ^= u64::from(*byte);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        mix_u64(state, h);
    }

    // Seed with both halves of the parent key independently; never fold them together.
    let mut key = u128::from(parent_key as u64 ^ 0x9e37_79b9_7f4a_7c15u64)
        | (u128::from((parent_key >> 64) as u64 ^ 0xd6e8_feb8_6659_fd93u64) << 64);
    for (name, declaration) in cascaded {
        let mut decl_hash = 0xcbf2_9ce4_8422_2325u128;
        hash_str(&mut decl_hash, name.as_str());
        hash_str(&mut decl_hash, declaration.raw_value.as_str());
        key ^= decl_hash.rotate_left(17).wrapping_mul(0x100_0000_01b3);
    }
    // The cache hashes declaration *values*, so a UA `max-width:100%` and an
    // author `max-width:100%` would otherwise share a key while carrying
    // different provenance (#2247).
    mix_u64(&mut key, u64::from(max_width_from_ua));
    // Final avalanche so distinct declaration sets cannot cancel by XOR alone.
    let mut out = key ^ (key >> 64);
    out ^= out.rotate_left(29).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    out ^= out.rotate_left(51).wrapping_mul(0x94d0_49bb_1331_11eb);
    out
}

/// Whether the cascade winner for a property came from the UA stylesheet.
///
/// W3C CSS Cascading 4 §6.1 assigns an origin to every declaration; `MatchedDeclaration`
/// keeps it as `origin_importance`, but `owned_declarations` drops it before
/// `ComputedStyle::compute`. Capture it here so the computed style can record provenance.
fn cascaded_from_ua(
    best: &HashMap<&str, MatchedDeclaration<'_>, FastBuildHasher>,
    property: &str,
) -> bool {
    best.get(property).is_some_and(|matched| {
        matches!(
            matched.origin_importance,
            CascadeOriginImportance::UserAgentNormal | CascadeOriginImportance::UserAgentImportant
        )
    })
}

/// Resolves the computed `background-color` for a `::backdrop` pseudo-element of `node_id`
/// when `node_id` is a modal `<dialog>` (`crate::state::is_modal(doc, node_id)`).
pub fn resolve_backdrop_color(
    doc: &Document,
    node_id: NodeId,
    collection: &StyleCollection,
    parent_style: Option<&crate::properties::ComputedStyle>,
    root_style: Option<&crate::properties::ComputedStyle>,
) -> Option<crate::properties::Color> {
    if !crate::state::is_modal(doc, node_id) || collection.backdrop_rules.is_empty() {
        return None;
    }
    let _match_scope = crate::selector::match_scope();
    let mut best_candidates: HashMap<&str, MatchedDeclaration<'_>, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);
    for rule in &collection.backdrop_rules {
        for selector in &rule.selectors.selectors {
            if !selector.matches_backdrop(doc, node_id) {
                continue;
            }
            let spec = selector.specificity();
            for (declaration_order, decl) in rule.declarations.iter().enumerate() {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity: spec,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                };
                match best_candidates.entry(decl.name.as_str()) {
                    std::collections::hash_map::Entry::Occupied(mut occupied) => {
                        if candidate > *occupied.get() {
                            occupied.insert(candidate);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        vacant.insert(candidate);
                    }
                }
            }
        }
    }
    if best_candidates.is_empty() {
        return None;
    }
    let cascaded = owned_declarations(best_candidates);
    let computed = crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
    Some(computed.background_color)
}

/// Resolves computed styles and pseudo-element (`::before` / `::after`) styles with a fully
/// specified [`MediaContext`].
pub fn resolve_document_styles_with_pseudos(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
) -> (
    HashMap<NodeId, crate::properties::ComputedStyle>,
    PseudoStyles,
) {
    let mut collection = crate::user_agent::create_user_agent_style_collection();
    collection.media = media;
    if let Some(css) = author_css {
        collection.add_css(css, Origin::Author);
    }
    resolve_document_styles_from_collection(doc, &collection)
}

/// Resolves computed styles and pseudo-element styles against a prebuilt [`StyleCollection`].
pub fn resolve_document_styles_from_collection(
    doc: &Document,
    collection: &StyleCollection,
) -> (
    HashMap<NodeId, crate::properties::ComputedStyle>,
    PseudoStyles,
) {
    resolve_document_styles_from_collection_inner(doc, collection, true)
}

/// Resolves computed styles with a fully specified [`MediaContext`] for `@media` evaluation.
pub fn resolve_document_styles_with_media(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    let mut collection = crate::user_agent::create_user_agent_style_collection();
    collection.media = media;
    if let Some(css) = author_css {
        collection.add_css(css, Origin::Author);
    }
    resolve_document_styles_from_collection_inner(doc, &collection, false).0
}

/// Maximum byte length parsed from a single inline `style="..."` attribute.
///
/// Bounding inline styles caps CPU and allocation cost on attacker-controlled documents;
/// multi-byte UTF-8 code points that straddle this limit are excluded by backing up to a
/// UTF-8 character boundary via `floor_char_boundary_utf8`.
pub const MAX_INLINE_STYLE_BYTES: usize = 64 * 1024;

/// Returns the greatest byte offset `<= max_bytes` on a UTF-8 `char` boundary in `text`.
#[inline]
pub(crate) fn floor_char_boundary_utf8(text: &str, max_bytes: usize) -> usize {
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Parses an inline `style="..."` attribute bounded to [`MAX_INLINE_STYLE_BYTES`] on a UTF-8
/// character boundary, returning unexpanded declarations.
pub fn parse_inline_declarations(style: &str) -> Vec<Declaration> {
    let bounded = if style.len() > MAX_INLINE_STYLE_BYTES {
        &style[..floor_char_boundary_utf8(style, MAX_INLINE_STYLE_BYTES)]
    } else {
        style
    };
    crate::parser::parse_list_of_declarations(bounded)
}

/// Parses and shorthand-expands an inline `style="..."` attribute bounded to
/// [`MAX_INLINE_STYLE_BYTES`] on a UTF-8 character boundary.
pub fn parse_inline_style(style: &str) -> Vec<Declaration> {
    expand_declarations(&parse_inline_declarations(style))
}

/// Builds a slot-index fallback map when `from_styles` comes from an earlier frame's
/// [`Document`] arena. Returns `None` when `from_styles` belongs to `doc` itself
/// (preserving strict generation safety).
pub fn previous_style_slot_lookup<'a>(
    doc: &Document,
    from_styles: &'a HashMap<NodeId, crate::properties::ComputedStyle>,
) -> Option<HashMap<usize, &'a crate::properties::ComputedStyle>> {
    if from_styles.is_empty() || from_styles.keys().any(|id| doc.owns(*id)) {
        None
    } else {
        Some(
            from_styles
                .iter()
                .map(|(id, style)| (id.slot(), style))
                .collect(),
        )
    }
}

/// Applies CSS transitions between `from_styles` and `styles` at `elapsed_ms`.
pub fn apply_transitions_to_styles(
    doc: &Document,
    from_styles: &HashMap<NodeId, crate::properties::ComputedStyle>,
    styles: &mut HashMap<NodeId, crate::properties::ComputedStyle>,
    elapsed_ms: f32,
) {
    if from_styles.is_empty() || !elapsed_ms.is_finite() {
        return;
    }
    let slot_lookup = previous_style_slot_lookup(doc, from_styles);
    let mut transitioned: Vec<(NodeId, crate::properties::ComputedStyle)> = Vec::new();
    for node_id in doc.descendants(doc.root()) {
        let from = match &slot_lookup {
            Some(by_slot) => by_slot.get(&node_id.slot()).copied(),
            None => from_styles.get(&node_id),
        };
        let (Some(from), Some(to)) = (from, styles.get(&node_id)) else {
            continue;
        };
        if to.transitions.is_empty() {
            continue;
        }
        let style = crate::animation_runtime::apply_transitions(from, to, elapsed_ms);
        if &style != to {
            transitioned.push((node_id, style));
        }
    }
    for (node_id, style) in transitioned {
        styles.insert(node_id, style);
    }
}

/// Applies CSS transitions between `from_pseudos` and `pseudo_styles` at `elapsed_ms`.
pub fn apply_transitions_to_pseudo_styles(
    from_pseudos: &PseudoStyles,
    pseudo_styles: &mut PseudoStyles,
    elapsed_ms: f32,
) {
    if from_pseudos.is_empty() || pseudo_styles.is_empty() || !elapsed_ms.is_finite() {
        return;
    }
    for (key, to) in pseudo_styles.iter_mut() {
        if to.transitions.is_empty() {
            continue;
        }
        let Some(from) = from_pseudos.get(key) else {
            continue;
        };
        *to = crate::animation_runtime::apply_transitions(from, to, elapsed_ms);
    }
}
