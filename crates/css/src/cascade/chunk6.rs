//! Temporal CSS style mutation: animation and transition application.
//!
//! Sized submodule of the `cascade` facade: the public functions are re-exported
//! by the parent, while the implementation stays behind that narrow interface.

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
