//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

pub(crate) fn resolve_pseudo_cascaded_with_candidates<'a>(
    doc: &Document,
    node_id: NodeId,
    pseudo: PseudoElement,
    collection: &'a StyleCollection,
    candidates: &[Candidate],
) -> HashMap<String, &'a Declaration> {
    let mut best = resolve_base_cascaded_properties_for(
        doc,
        node_id,
        Some(pseudo),
        collection,
        None,
        candidates,
    );
    merge_container_rules_for(doc, node_id, Some(pseudo), collection, &mut best);
    apply_revert_layer_for(
        doc,
        node_id,
        Some(pseudo),
        collection,
        candidates,
        &mut best,
    );
    owned_declarations(best)
}

/// Shared cascade resolution over the candidate rules selected by the index.
///
/// `candidates` is a superset of the selectors that can match `node_id`, in ascending source
/// order (the index only prunes; matching still decides). Passing every `(rule, selector)`
/// pair yields the exact unindexed behaviour, which the equivalence tests use as reference.
pub(crate) fn resolve_base_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
    candidates: &[Candidate],
) -> HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher> {
    resolve_base_cascaded_properties_for(
        doc,
        node_id,
        None,
        collection,
        inline_declarations,
        candidates,
    )
}

pub(crate) fn resolve_base_cascaded_properties_for<'a>(
    doc: &Document,
    node_id: NodeId,
    pseudo: Option<PseudoElement>,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
    candidates: &[Candidate],
) -> HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher> {
    // One selector-matching budget for this element: every candidate selector shares it.
    let _match_scope = crate::selector::match_scope();
    let mut best_candidates: HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);

    // 1. Match stylesheet rules (indexed candidates only; the index guarantees a superset).
    //    A rule with several matching selectors contributes each of them; the cascade compares
    //    their specificities with the same total order, so the winner equals the rule-level
    //    "maximum matching specificity" semantics.
    for candidate in candidates {
        let rule = &collection.rules[candidate.rule as usize];
        let selector = &rule.selectors.selectors[candidate.selector as usize];
        if selector_matches(selector, doc, node_id, pseudo) {
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

    // 2. Process inline styles from style attribute (Author origin with high specificity)
    if pseudo.is_none()
        && let Some(inline_decls) = inline_declarations
    {
        let inline_specificity = Specificity::new(100_000, 0, 0);
        let inline_source_order = collection.next_source_order + 10_000;
        for (declaration_order, decl) in inline_decls.iter().enumerate() {
            let origin_importance = CascadeOriginImportance::new(Origin::Author, decl.important);
            let candidate = MatchedDeclaration {
                declaration: decl,
                origin_importance,
                layer_rank: UNLAYERED_LAYER,
                specificity: inline_specificity,
                source_order: inline_source_order,
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

    best_candidates
}

/// Reference implementation used by cascade-equivalence tests (no `@container` rules).
#[cfg(test)]
pub(crate) fn resolve_cascaded_properties_from_candidates<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
    candidates: &[Candidate],
) -> HashMap<String, &'a Declaration> {
    owned_declarations(resolve_base_cascaded_properties(
        doc,
        node_id,
        collection,
        inline_declarations,
        candidates,
    ))
}

/// Applies the collection's `@container` rules to `best` (already resolved base cascade).
pub(crate) fn merge_container_rules_for<'a>(
    doc: &Document,
    node_id: NodeId,
    pseudo: Option<PseudoElement>,
    collection: &'a StyleCollection,
    best: &mut HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) {
    // Documents without `@container` rules (the vast majority) skip the loop entirely.
    if collection.container_rules.is_empty() {
        return;
    }
    for container_rule in &collection.container_rules {
        let rule = &container_rule.rule;
        for selector in &rule.selectors.selectors {
            if !selector_matches(selector, doc, node_id, pseudo) {
                continue;
            }
            let Some((size, container_style)) =
                crate::container::matching_size(doc, node_id, &container_rule.query, collection)
            else {
                continue;
            };
            if !crate::container::evaluate(&container_rule.query, &size, &container_style) {
                continue;
            }
            let specificity = selector.specificity();
            for (declaration_order, decl) in rule.declarations.iter().enumerate() {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                };
                match best.entry(decl.name.as_str()) {
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
}

pub(crate) fn is_revert_layer(declaration: &Declaration) -> bool {
    declaration
        .raw_value
        .trim()
        .eq_ignore_ascii_case("revert-layer")
}

pub(crate) fn is_important(origin_importance: CascadeOriginImportance) -> bool {
    matches!(
        origin_importance,
        CascadeOriginImportance::AuthorImportant
            | CascadeOriginImportance::UserImportant
            | CascadeOriginImportance::UserAgentImportant
    )
}

/// Applies `revert-layer`: the winner rolls back to the previous cascade layer (not the UA
/// origin). Candidates from the same layer are skipped; a missing previous layer removes the
/// property (initial/inherited value).
pub(crate) fn apply_revert_layer_for<'a>(
    doc: &Document,
    node_id: NodeId,
    pseudo: Option<PseudoElement>,
    collection: &'a StyleCollection,
    candidates: &[Candidate],
    best: &mut HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) {
    // `revert-layer` is rare: without any such declaration the per-element scan (which
    // allocates a Vec and string-compares every winning declaration) is pure overhead.
    // The flag is only trusted when the index is in sync; a `rules` vector mutated
    // directly (documented fallback path) keeps the slow, always-correct scan.
    if !collection.has_revert_layer && collection.index.indexed_rules == collection.rules.len() {
        return;
    }
    let losers: Vec<&'a str> = best
        .iter()
        .filter(|(_, matched)| is_revert_layer(matched.declaration))
        .map(|(name, _)| *name)
        .collect();
    for property in losers {
        let Some(winner) = best.get(property).cloned() else {
            continue;
        };
        let important = is_important(winner.origin_importance);
        let mut matched: Vec<MatchedDeclaration<'a>> = Vec::new();
        // Re-evaluate only the index's candidates (a superset of the matching selectors)
        // instead of scanning every rule × selector of the collection: `revert-layer` used
        // to disable the rule index for each affected property.
        for candidate in candidates {
            let Some(rule) = collection.rules.get(candidate.rule as usize) else {
                continue;
            };
            let Some(selector) = rule.selectors.selectors.get(candidate.selector as usize) else {
                continue;
            };
            if !selector_matches(selector, doc, node_id, pseudo) {
                continue;
            }
            let specificity = selector.specificity();
            for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                if declaration.name != property {
                    continue;
                }
                let origin_importance =
                    CascadeOriginImportance::new(rule.origin, declaration.important);
                if origin_importance != winner.origin_importance {
                    continue;
                }
                matched.push(MatchedDeclaration {
                    declaration,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                });
            }
        }
        for container_rule in &collection.container_rules {
            let rule = &container_rule.rule;
            let Some((size, container_style)) =
                crate::container::matching_size(doc, node_id, &container_rule.query, collection)
            else {
                continue;
            };
            if !crate::container::evaluate(&container_rule.query, &size, &container_style) {
                continue;
            }
            for selector in &rule.selectors.selectors {
                if !selector_matches(selector, doc, node_id, pseudo) {
                    continue;
                }
                let specificity = selector.specificity();
                for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                    if declaration.name != property {
                        continue;
                    }
                    let origin_importance =
                        CascadeOriginImportance::new(rule.origin, declaration.important);
                    if origin_importance != winner.origin_importance {
                        continue;
                    }
                    matched.push(MatchedDeclaration {
                        declaration,
                        origin_importance,
                        layer_rank: rule.layer_rank,
                        specificity,
                        source_order: rule.source_order,
                        declaration_order: declaration_order as u32,
                    });
                }
            }
        }
        matched.sort_by(|a, b| b.cmp(a));
        let mut replacement = None;
        for candidate in matched {
            if candidate.layer_rank == winner.layer_rank {
                continue;
            }
            let lower_priority = if important {
                candidate.layer_rank > winner.layer_rank
            } else {
                candidate.layer_rank < winner.layer_rank
            };
            if lower_priority {
                replacement = Some(candidate);
                break;
            }
        }
        match replacement {
            Some(candidate) if !is_revert_layer(candidate.declaration) => {
                best.insert(property, candidate);
            }
            _ => {
                best.remove(property);
            }
        }
    }
}

pub(crate) fn owned_declarations<'a>(
    best: HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) -> HashMap<String, &'a Declaration> {
    best.into_iter()
        .map(|(name, matched)| (name.to_string(), matched.declaration))
        .collect()
}

/// Resolves only the unconditional cascade (no `@container` rules); used by container size
/// prediction so it cannot recurse through itself.
pub(crate) fn resolve_cascaded_properties_base<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut candidates = Vec::new();
    collection.candidate_selectors(doc, node_id, &mut candidates);
    owned_declarations(resolve_base_cascaded_properties(
        doc,
        node_id,
        collection,
        inline_declarations,
        &candidates,
    ))
}

/// Resolves computed styles with an explicit interaction [`crate::StateContext`] for dynamic
/// pseudo-classes (`:focus`, `:hover`, `:checked`, ...). `render`/shell should call this so a
/// focused/edited form re-resolves with the host state; static documents keep using
/// [`resolve_document_styles_with_media`].
pub fn resolve_document_styles_with_media_and_state(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
    state: &crate::StateContext,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    crate::state::with_state(*state, || {
        resolve_document_styles_with_media(doc, author_css, media)
    })
}
