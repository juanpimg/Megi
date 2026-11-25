//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

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
            let hints = presentational_declarations(doc, node_id);

            collection.candidate_selectors(doc, node_id, &mut candidates);
            let mut best = resolve_base_cascaded_properties_for(
                doc,
                node_id,
                None,
                collection,
                inline_decls.as_deref(),
                Some(&hints),
                &candidates,
            );
            merge_container_rules_for(doc, node_id, None, collection, &mut best);
            apply_revert_for(doc, node_id, None, collection, &candidates, &mut best);
            apply_revert_layer_for(doc, node_id, None, collection, &candidates, &mut best);
            let max_width_from_ua = cascaded_from_ua(&best, "max-width");
            let cascaded = owned_declarations(best);

            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);
            // Inline `style="..."` and presentational-hint declarations live in a
            // per-element buffer whose memory could be reused by the next element, so
            // they are excluded from the cache (both are rare); everything else lives
            // in the immutable StyleCollection, where declaration addresses are stable.
            let is_modal = crate::state::is_modal(doc, node_id);
            let cacheable = inline_decls.is_none() && hints.is_empty() && !is_modal;
            let parent_key = doc
                .node(node_id)
                .parent
                .and_then(|parent| style_keys.get(&parent).copied())
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            let cache_key = style_cache_key(parent_key, &cascaded, max_width_from_ua);
            if cacheable && let Some(cached) = style_cache.get(&cache_key) {
                styles.insert(node_id, cached.clone());
                style_keys.insert(node_id, cache_key);
            } else {
                let mut computed =
                    crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
                computed.max_width_from_ua = max_width_from_ua;
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
