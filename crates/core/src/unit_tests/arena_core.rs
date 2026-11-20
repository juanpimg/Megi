//! Arena identity, handle validity and core mutation tests.

use crate::*;

#[test]
fn corrupt_free_list_entry_is_skipped_in_release_too() {
    let mut doc = Document::new();
    let a = doc.create_text_node("a");
    let occupied = a.index();
    // Inject a corrupt entry pointing at a live slot (#1467).
    doc.free_list.push(occupied as u32);
    let b = doc.create_text_node("b");
    assert_ne!(
        b.index(),
        occupied,
        "allocation must never hand out an occupied slot"
    );
    assert_eq!(doc.node(a).as_text(), Some("a"));
}

#[test]
fn test_node_id_niche_optimization_is_4_bytes() {
    assert_eq!(std::mem::size_of::<NodeId>(), 4);
    assert_eq!(std::mem::size_of::<Option<NodeId>>(), 4);
}

#[test]
fn test_append_insert_before_and_detach_preserve_sibling_invariants() {
    let mut doc = Document::new();
    let root = doc.root();

    let div = doc.create_element(Namespace::Html, "div", vec![]);
    let p1 = doc.create_element(Namespace::Html, "p", vec![]);
    let p2 = doc.create_element(Namespace::Html, "p", vec![]);
    let span = doc.create_element(Namespace::Html, "span", vec![]);

    doc.append_child(root, div);
    doc.append_child(div, p1);
    doc.append_child(div, p2);
    doc.insert_before(p2, span);

    let children: Vec<NodeId> = doc.children(div).collect();
    assert_eq!(children, vec![p1, span, p2]);
    assert_eq!(doc.node(div).first_child, Some(p1));
    assert_eq!(doc.node(div).last_child, Some(p2));
    assert_eq!(doc.node(span).prev_sibling, Some(p1));
    assert_eq!(doc.node(span).next_sibling, Some(p2));

    // Detach middle child and verify O(1) splice
    doc.detach(span);
    let children_after: Vec<NodeId> = doc.children(div).collect();
    assert_eq!(children_after, vec![p1, p2]);
    assert_eq!(doc.node(p1).next_sibling, Some(p2));
    assert_eq!(doc.node(p2).prev_sibling, Some(p1));
    assert_eq!(doc.node(span).parent, None);
}

#[test]
fn test_append_text_and_insert_text_before_coalesce_adjacent_text_nodes() {
    let mut doc = Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let table = doc.create_element(Namespace::Html, "table", vec![]);
    doc.append_child(doc.root(), body);

    let t1 = doc.append_text(body, "Hello").unwrap();
    let t2 = doc.append_text(body, " ").unwrap();
    doc.append_child(body, table);

    // t1 and t2 must have coalesced into the same Text node
    assert_eq!(t1, t2);

    // Foster parenting simulation: insert text before <table>
    let t3 = doc.insert_text_before(table, "World!").unwrap();
    assert_eq!(t1, t3);
    assert_eq!(
        doc.node(t1).kind,
        NodeKind::Text("Hello World!".to_string())
    );
}

#[test]
fn test_template_element_allocates_inert_document_fragment() {
    let mut doc = Document::new();
    let tmpl = doc.create_element(Namespace::Html, "template", vec![]);
    let frag_id = doc
        .node(tmpl)
        .as_element()
        .and_then(|el| el.template_contents)
        .expect("HTML <template> must have template_contents DocumentFragment");
    assert_eq!(doc.node(frag_id).kind, NodeKind::DocumentFragment);
}

#[test]
fn reparent_children_rejects_cycles_without_partial_moves() {
    // #683: moving a parent's children under a target that sits inside the moved set used
    // to move part of them and report success (the API returned `()`).
    let mut doc = Document::new();
    let root = doc.root();
    let a = doc.create_element(Namespace::Html, "div", vec![]);
    let b = doc.create_element(Namespace::Html, "div", vec![]);
    let d = doc.create_element(Namespace::Html, "div", vec![]);
    let c = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(root, a);
    doc.append_child(a, b);
    doc.append_child(a, c);
    doc.append_child(b, d);

    assert!(
        !doc.reparent_children(a, d),
        "moving A's children under a descendant of A must be rejected"
    );
    assert_eq!(doc.node(a).first_child, Some(b), "A must keep its children");
    assert_eq!(doc.node(b).first_child, Some(d));
    assert_eq!(
        doc.node(d).first_child,
        None,
        "nothing may be moved under D"
    );

    // The trivial variant: the target is a direct child, so the move is impossible too.
    assert!(!doc.reparent_children(a, b));
    assert_eq!(doc.node(a).first_child, Some(b));
    assert_eq!(doc.node(b).first_child, Some(d));

    // A legal move still works and reports success.
    let target = doc.create_element(Namespace::Html, "section", vec![]);
    doc.append_child(root, target);
    assert!(doc.reparent_children(a, target));
    assert_eq!(doc.node(a).first_child, None);
    assert_eq!(doc.node(target).first_child, Some(b));
    assert_eq!(doc.node(target).last_child, Some(c));
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn insert_node_fragment_expansion_is_all_or_nothing() {
    // #685: a fragment whose second child cannot move must leave both the fragment and the
    // destination untouched instead of splitting the fragment.
    let mut doc = Document::new();
    let root = doc.root();
    let anc = doc.create_element(Namespace::Html, "div", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(root, anc);
    doc.append_child(anc, p);

    let fragment = doc.create_document_fragment();
    let x = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(fragment, x);
    doc.append_child(fragment, anc);

    let err = doc
        .insert_node(p, fragment, None)
        .expect_err("the fragment holds an ancestor of the destination");
    assert!(matches!(err, DomError::HierarchyRequest { .. }), "{err:?}");
    assert_eq!(
        doc.node(fragment).first_child,
        Some(x),
        "the first child must not be moved when a later one fails"
    );
    assert_eq!(doc.node(x).parent, Some(fragment));
    assert_eq!(doc.node(p).first_child, None);
    assert_eq!(doc.node(anc).parent, Some(fragment));
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_reparent_children_moves_entire_child_chain() {
    let mut doc = Document::new();
    let old_p = doc.create_element(Namespace::Html, "b", vec![]);
    let new_p = doc.create_element(Namespace::Html, "i", vec![]);
    let c1 = doc.create_element(Namespace::Html, "span", vec![]);
    let c2 = doc.create_element(Namespace::Html, "em", vec![]);

    doc.append_child(old_p, c1);
    doc.append_child(old_p, c2);
    doc.reparent_children(old_p, new_p);

    assert_eq!(doc.children(old_p).count(), 0);
    let moved: Vec<NodeId> = doc.children(new_p).collect();
    assert_eq!(moved, vec![c1, c2]);
    assert_eq!(doc.node(c1).parent, Some(new_p));
    assert_eq!(doc.node(c2).parent, Some(new_p));
}

#[test]
fn test_append_child_rejects_self_and_ancestor_cycles_in_release() {
    let mut doc = Document::new();
    let root = doc.root();
    let outer = doc.create_element(Namespace::Html, "div", vec![]);
    let inner = doc.create_element(Namespace::Html, "p", vec![]);
    let grandchild = doc.create_element(Namespace::Html, "span", vec![]);
    assert!(doc.append_child(root, outer));
    assert!(doc.append_child(outer, inner));
    assert!(doc.append_child(inner, grandchild));

    // Self-insertion must be rejected, not corrupt the arena.
    assert!(!doc.append_child(inner, inner));
    assert_eq!(doc.node(inner).parent, Some(outer));

    // Ancestor under descendant must be rejected (would create a cycle and make
    // `ancestors()` iterate forever).
    assert!(!doc.append_child(grandchild, outer));
    assert!(!doc.append_child(inner, outer));
    assert_eq!(doc.node(outer).parent, Some(root));
    assert_eq!(doc.node(outer).first_child, Some(inner));
    assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![outer]);

    // `ancestors()` terminates on the intact tree.
    let ancestors: Vec<NodeId> = doc.ancestors(grandchild).collect();
    assert_eq!(ancestors, vec![inner, outer, root]);
}

#[test]
fn test_insert_before_detached_reference_does_not_panic() {
    let mut doc = Document::new();
    let root = doc.root();
    let detached_ref = doc.create_element(Namespace::Html, "b", vec![]);
    let node = doc.create_element(Namespace::Html, "i", vec![]);
    let sibling = doc.create_element(Namespace::Html, "u", vec![]);
    doc.append_child(root, sibling);

    // Reference sibling without a parent: clean early return, no panic.
    assert!(!doc.insert_before(detached_ref, node));
    assert_eq!(doc.node(node).parent, None);
    assert_eq!(doc.node(sibling).parent, Some(root));

    // Self-insertion and ancestor-into-descendant are rejected too.
    assert!(doc.insert_before(sibling, node));
    assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![node, sibling]);
    assert!(!doc.insert_before(node, node));
    let nested = doc.create_element(Namespace::Html, "div", vec![]);
    let child = doc.create_element(Namespace::Html, "em", vec![]);
    assert!(doc.append_child(nested, child));
    assert!(doc.append_child(root, nested));
    // `nested` is an ancestor of `child`; inserting it before `child` would cycle.
    assert!(!doc.insert_before(child, nested));
    assert_eq!(doc.node(nested).parent, Some(root));
}

#[test]
fn test_clone_node_template_gets_independent_contents() {
    let mut doc = Document::new();
    let tmpl = doc.create_element(Namespace::Html, "template", vec![]);
    let original_frag = doc
        .node(tmpl)
        .as_element()
        .and_then(|el| el.template_contents)
        .expect("template has contents fragment");
    let span = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(original_frag, span);
    let _ = doc.append_text(span, "Inert");

    let cloned = doc.clone_node(tmpl, false).unwrap();
    assert_ne!(cloned, tmpl);
    let cloned_frag = doc
        .node(cloned)
        .as_element()
        .and_then(|el| el.template_contents)
        .expect("clone has contents fragment");
    assert_ne!(
        cloned_frag, original_frag,
        "template contents must not be shared"
    );

    // Template contents are cloned deeply only when `deep` is true; a shallow
    // clone creates an empty inert fragment (#1111).
    let cloned_children: Vec<NodeId> = doc.children(cloned_frag).collect();
    assert_eq!(cloned_children.len(), 0);

    // The original template is untouched.
    assert_eq!(doc.children(original_frag).collect::<Vec<_>>(), vec![span]);
}

#[test]
fn test_try_node_is_fallible_for_out_of_range_ids() {
    let mut doc = Document::new();
    let out_of_range = NodeId::from_index(9999);
    assert!(doc.try_node(out_of_range).is_none());
    assert!(doc.try_node_mut(out_of_range).is_none());
    assert!(doc.try_node(doc.root()).is_some());
}
#[test]
fn test_cross_document_handle_is_rejected() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let mut doc_a = Document::new();
    let mut doc_b = Document::new();
    assert_ne!(doc_a.arena_id(), doc_b.arena_id());
    let a = doc_a.create_element(Namespace::Html, "div", vec![]);
    let b = doc_b.create_element(Namespace::Html, "div", vec![]);
    assert!(doc_a.owns(a));
    assert!(!doc_a.owns(b), "foreign handle must not be owned");
    assert!(doc_a.try_node(b).is_none());
    let payload = catch_unwind(AssertUnwindSafe(|| {
        let _ = doc_a.node(b);
    }))
    .expect_err("reading a foreign NodeId must panic");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default();
    assert!(
        message.contains("does not belong to this Document"),
        "unexpected panic message: {message}"
    );
    // Same-document lookups keep working.
    assert_eq!(doc_a.node(a).id, a);
}

#[test]
fn test_slot_reuse_invalidates_stale_handles() {
    let mut doc = Document::new();
    let root = doc.root();
    let first = doc.create_node(NodeKind::Text("old".to_owned()));
    doc.append_child(root, first);
    assert_eq!(doc.generation(first), Some(0));
    assert!(doc.remove_node(first));

    assert!(
        doc.try_node(first).is_none(),
        "stale handle must not resolve"
    );
    assert!(!doc.owns(first));
    let reused = doc.create_node(NodeKind::Text("new".to_owned()));
    assert_eq!(reused.index(), first.index(), "free slot must be reused");
    assert_ne!(reused, first, "reused slot gets a new generation");
    assert_eq!(doc.generation(reused), Some(1));
    assert!(
        doc.try_node(first).is_none(),
        "stale handle must stay stale after reuse"
    );
    assert_eq!(doc.try_node(reused).and_then(Node::as_text), Some("new"));
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_remove_node_reattaches_children_in_place() {
    let mut doc = Document::new();
    let root = doc.root();
    let wrapper = doc.create_element(Namespace::Html, "div", vec![]);
    let a = doc.create_element(Namespace::Html, "span", vec![]);
    let b = doc.create_element(Namespace::Html, "span", vec![]);
    let sibling = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(root, wrapper);
    doc.append_child(wrapper, a);
    doc.append_child(wrapper, b);
    doc.append_child(root, sibling);

    assert!(doc.remove_node(wrapper));
    assert!(doc.try_node(wrapper).is_none());
    assert_eq!(doc.children(root).collect::<Vec<_>>(), vec![a, b, sibling]);
    assert_eq!(doc.node(a).parent, Some(root));
    assert_eq!(doc.node(b).parent, Some(root));
    assert_eq!(doc.len(), 4);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_remove_subtree_frees_all_descendants_and_reuses_slots() {
    let mut doc = Document::new();
    let root = doc.root();
    let mut chain = Vec::new();
    let mut parent = root;
    for _ in 0..1_000 {
        let element = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(parent, element);
        chain.push(element);
        parent = element;
    }
    let allocated_before = doc.allocated();
    assert_eq!(doc.len(), 1_001);

    assert_eq!(doc.remove_subtree(chain[0]), 1_000);
    assert_eq!(doc.len(), 1);
    assert_eq!(doc.free_slots(), 1_000);
    assert_eq!(doc.allocated(), allocated_before);
    assert!(chain.iter().all(|id| doc.try_node(*id).is_none()));
    assert!(doc.validate_invariants().is_ok());

    // Free slots are reused instead of growing the arena again.
    for _ in 0..1_000 {
        let element = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(root, element);
    }
    assert_eq!(doc.len(), 1_001);
    assert_eq!(doc.allocated(), allocated_before);
    assert_eq!(doc.free_slots(), 0);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_remove_template_frees_inert_contents() {
    let mut doc = Document::new();
    let root = doc.root();
    let template = doc.create_element(Namespace::Html, "template", vec![]);
    let fragment = doc
        .node(template)
        .as_element()
        .and_then(|element| element.template_contents)
        .expect("template has contents");
    let inner = doc.create_element(Namespace::Html, "span", vec![]);
    doc.append_child(fragment, inner);
    doc.append_child(root, template);

    assert!(doc.remove_node(template));
    assert!(doc.try_node(template).is_none());
    assert!(doc.try_node(fragment).is_none(), "contents fragment freed");
    assert!(doc.try_node(inner).is_none(), "contents descendants freed");
    assert_eq!(doc.len(), 1);
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn test_remove_root_is_rejected() {
    let mut doc = Document::new();
    let root = doc.root();
    assert!(!doc.remove_node(root));
    assert_eq!(doc.remove_subtree(root), 0);
    assert!(doc.owns(root));
    assert!(doc.validate_invariants().is_ok());
}

#[test]
fn from_index_out_of_range_never_panics_and_does_not_resolve() {
    // #1732: `pack` masks `slot + 1` into 20 bits, so indices congruent to MAX_SLOTS
    // (mod 2^20) wrapped to the zero handle and panicked, and usize::MAX overflowed in debug.
    // They must instead produce an invalid handle that `try_node` rejects.
    let invalid = [MAX_SLOTS, 2 * MAX_SLOTS - 1, usize::MAX];
    let mut doc = Document::new();
    for idx in invalid {
        let id = NodeId::from_index(idx);
        assert_ne!(
            id,
            NodeId::from_index(0),
            "idx {idx} must not be the zero-slot handle"
        );
        assert!(doc.try_node(id).is_none(), "idx {idx} must not resolve");
        assert!(
            doc.try_node_mut(id).is_none(),
            "idx {idx} must not resolve mutably"
        );
    }
    // The highest representable slot still round-trips.
    assert_eq!(NodeId::from_index(MAX_SLOTS - 1).index(), MAX_SLOTS - 1);
}
