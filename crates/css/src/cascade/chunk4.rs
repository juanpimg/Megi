//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

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
pub(crate) fn style_cache_key(parent_key: u128, cascaded: &HashMap<String, &Declaration>) -> u128 {
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
    // Final avalanche so distinct declaration sets cannot cancel by XOR alone.
    let mut out = key ^ (key >> 64);
    out ^= out.rotate_left(29).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    out ^= out.rotate_left(51).wrapping_mul(0x94d0_49bb_1331_11eb);
    out
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

pub(crate) fn resolve_document_styles_from_collection_inner(
    doc: &Document,
    collection: &StyleCollection,
    include_pseudos: bool,
) -> (
    HashMap<NodeId, crate::properties::ComputedStyle>,
    PseudoStyles,
) {
    // One sibling-index cache for the whole pass: `:nth-*` matching becomes O(1) per element
    // instead of scanning the sibling chain.
    let _sibling_scope = crate::selector::document_scope();

    let mut styles: HashMap<NodeId, crate::properties::ComputedStyle> = HashMap::new();
    let mut pseudo_styles: PseudoStyles = HashMap::new();
    let mut style_cache: HashMap<u128, crate::properties::ComputedStyle, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);
    let mut style_seen: HashMap<u128, u8, FastBuildHasher> = HashMap::with_hasher(FastBuildHasher);
    let mut style_keys: HashMap<NodeId, u128, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);
    let mut candidates: Vec<Candidate> = Vec::new();
    let check_pseudos = include_pseudos && collection.may_have_pseudo_rules();
    let root = doc.root();

    // Collect elements in top-down pre-order traversal
    let mut stack = Vec::new();
    let mut curr = doc.node(root).first_child;
    while let Some(child_id) = curr {
        stack.push(child_id);
        curr = doc.node(child_id).next_sibling;
    }

    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element() {
            let inline_decls = el.get_attribute("style").map(parse_inline_style);

            collection.candidate_selectors(doc, node_id, &mut candidates);
            let mut best = resolve_base_cascaded_properties_for(
                doc,
                node_id,
                None,
                collection,
                inline_decls.as_deref(),
                &candidates,
            );
            merge_container_rules_for(doc, node_id, None, collection, &mut best);
            apply_revert_for(doc, node_id, None, collection, &candidates, &mut best);
            apply_revert_layer_for(doc, node_id, None, collection, &candidates, &mut best);
            let cascaded = owned_declarations(best);

            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);
            // Inline `style="..."` declarations live in a per-element buffer whose memory
            // could be reused by the next element, so they are excluded from the
            // pointer-keyed cache (they are also rare); everything else lives in the
            // immutable StyleCollection, where declaration addresses are stable.
            let is_modal = crate::state::is_modal(doc, node_id);
            let cacheable = inline_decls.is_none() && !is_modal;
            let parent_key = doc
                .node(node_id)
                .parent
                .and_then(|parent| style_keys.get(&parent).copied())
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            let cache_key = style_cache_key(parent_key, &cascaded);
            if cacheable && let Some(cached) = style_cache.get(&cache_key) {
                styles.insert(node_id, cached.clone());
                style_keys.insert(node_id, cache_key);
            } else {
                let mut computed =
                    crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
                if is_modal {
                    computed.set_backdrop_color(resolve_backdrop_color(
                        doc,
                        node_id,
                        collection,
                        parent_style,
                        root_style,
                    ));
                }
                if cacheable {
                    // Only repeated declaration sets are cached; unique elements pay a
                    // cheap hash/counter instead of a full ComputedStyle clone, and the
                    // miss path never clones (the computed value moves into `styles`).
                    let seen = style_seen.entry(cache_key).or_insert(0);
                    *seen = seen.saturating_add(1);
                    if *seen >= 2 && style_cache.len() < 1024 {
                        style_cache.insert(cache_key, computed.clone());
                    }
                }
                styles.insert(node_id, computed);
                if cacheable {
                    style_keys.insert(node_id, cache_key);
                } else {
                    let mut hash = 0xcbf2_9ce4_8422_2325u64;
                    for &byte in node_id.get().to_le_bytes().iter() {
                        hash ^= u64::from(byte);
                        hash = hash.wrapping_mul(0x100_0000_01b3);
                    }
                    style_keys.insert(node_id, u128::from(hash) << 64 | u128::from(hash ^ 0xa5a5));
                }
            }

            if check_pseudos {
                let originating_style = styles.get(&node_id);
                let pseudo_root_style = styles.get(&root);
                for pseudo in [PseudoElement::Before, PseudoElement::After] {
                    let pseudo_cascaded = resolve_pseudo_cascaded_with_candidates(
                        doc,
                        node_id,
                        pseudo,
                        collection,
                        &candidates,
                    );
                    if pseudo_cascaded.is_empty() {
                        continue;
                    }
                    let computed_pseudo = crate::properties::ComputedStyle::compute(
                        &pseudo_cascaded,
                        originating_style,
                        pseudo_root_style,
                    );
                    if computed_pseudo.content.generates_box() {
                        pseudo_styles.insert((node_id, pseudo), computed_pseudo);
                    }
                }
            }
        }

        // Push children in reverse order (last child first) so the first child is popped
        // first; this avoids a temporary allocation per node on the hot resolution path.
        let mut child = doc.node(node_id).last_child;
        while let Some(c) = child {
            stack.push(c);
            child = doc.node(c).prev_sibling;
        }
    }

    (styles, pseudo_styles)
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
