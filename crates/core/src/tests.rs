//! Unit tests for the document-owned arena.

#[cfg(test)]
mod tests {
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

    use crate::*;

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
    fn test_deep_iterators_are_iterative_and_sized() {
        let mut doc = Document::new();
        let mut parent = doc.root();
        let mut ids = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(parent, element);
            ids.push(element);
            parent = element;
        }
        assert_eq!(doc.descendants(doc.root()).count(), 100_000);
        assert_eq!(doc.descendants_inclusive(doc.root()).count(), 100_001);
        assert_eq!(doc.ancestors(ids[99_999]).count(), 100_000);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_iterators_terminate_on_corrupted_cycles() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(root, a);
        doc.append_child(a, b);
        // Corrupt the parent chain into a cycle b -> a -> b.
        doc.nodes[a.index()].parent = Some(b);
        assert!(doc.validate_invariants().is_err());
        assert!(doc.ancestors(b).count() <= doc.len() + 1);
        assert!(doc.ancestors(a).count() <= doc.len() + 1);
    }

    #[test]
    fn test_validate_invariants_detects_corruption() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(root, a);
        doc.append_child(a, b);
        assert!(doc.validate_invariants().is_ok());

        // Sibling asymmetry + broken last_child chain.
        doc.nodes[a.index()].next_sibling = Some(b);
        assert!(doc.validate_invariants().is_err());
        doc.nodes[a.index()].next_sibling = None;
        assert!(doc.validate_invariants().is_ok());

        // Child does not point back to its parent.
        doc.nodes[b.index()].parent = Some(root);
        assert!(doc.validate_invariants().is_err());
        doc.nodes[b.index()].parent = Some(a);
        assert!(doc.validate_invariants().is_ok());

        // A detached node (parent == None) is a valid intermediate state for the parser.
        let detached = doc.create_element(Namespace::Html, "p", vec![]);
        assert!(doc.validate_invariants().is_ok());
        doc.append_child(root, detached);
        assert!(doc.validate_invariants().is_ok());

        // Broken free-list bookkeeping: a freed slot marked occupied again.
        let victim = doc.create_element(Namespace::Html, "em", vec![]);
        doc.append_child(root, victim);
        assert!(doc.remove_node(victim));
        let victim_slot = victim.index();
        doc.occupied[victim_slot] = true;
        assert!(doc.validate_invariants().is_err());
        doc.occupied[victim_slot] = false;
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_validate_invariants_rejects_children_under_non_containers() {
        let mut doc = Document::new();
        let root = doc.root();
        let text = doc.create_text_node("hello");
        let child = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(root, text);
        assert!(doc.validate_invariants().is_ok());

        // The public APIs reject this (`CannotHaveChildren`), so build the corruption the way
        // a broken arena import would: link a child under a Text node (#385).
        doc.nodes[text.index()].first_child = Some(child);
        doc.nodes[text.index()].last_child = Some(child);
        doc.nodes[child.index()].parent = Some(text);
        let error = doc
            .validate_invariants()
            .expect_err("children under a Text node");
        assert!(
            error.contains("non-container node holds children"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_stats_and_estimate_bytes() {
        let mut doc = Document::new();
        let root = doc.root();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.allocated(), 1);
        assert_eq!(doc.free_slots(), 0);
        let empty_estimate = doc.estimate_bytes();
        assert!(empty_estimate >= std::mem::size_of::<Document>());

        let mut ids = Vec::new();
        for index in 0..32 {
            let element = doc.create_element(
                Namespace::Html,
                "div",
                vec![Attribute::new("id", format!("n{index}"))],
            );
            doc.append_child(root, element);
            ids.push(element);
        }
        assert_eq!(doc.len(), 33);
        assert_eq!(doc.allocated(), 33);
        assert_eq!(doc.free_slots(), 0);
        let populated_estimate = doc.estimate_bytes();
        assert!(populated_estimate > empty_estimate);

        for id in ids.iter().take(10) {
            assert!(doc.remove_node(*id));
        }
        assert_eq!(doc.len(), 23);
        assert_eq!(doc.free_slots(), 10);
        assert_eq!(doc.allocated(), 33);
        assert!(
            doc.estimate_bytes() < populated_estimate,
            "removing payloads must lower the estimate"
        );

        for _ in 0..10 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(root, element);
        }
        assert_eq!(doc.len(), 33);
        assert_eq!(doc.allocated(), 33, "reused slots, no arena growth");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_many_create_remove_cycles_keep_invariants() {
        let mut doc = Document::new();
        let root = doc.root();
        for round in 0..2_000 {
            let element = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(root, element);
            if round % 2 == 0 {
                assert!(doc.remove_node(element));
            }
        }
        assert!(doc.validate_invariants().is_ok());
        assert!(doc.allocated() <= 2_002);
    }
    #[test]
    fn test_document_element_head_body_and_lookups() {
        let mut doc = Document::new();
        let root = doc.root();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let head = doc.create_element(Namespace::Html, "head", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(root, html);
        doc.append_child(html, head);
        doc.append_child(html, body);
        let nav = doc.create_element(
            Namespace::Html,
            "nav",
            vec![
                Attribute::new("id", "main-nav"),
                Attribute::new("class", "top primary"),
            ],
        );
        let p = doc.create_element(Namespace::Html, "p", vec![]);
        let svg = doc.create_element(Namespace::Svg, "circle", vec![]);
        doc.append_child(body, nav);
        doc.append_child(body, p);
        doc.append_child(body, svg);

        assert_eq!(doc.document_element(), Some(html));
        assert_eq!(doc.head(), Some(head));
        assert_eq!(doc.body(), Some(body));
        assert_eq!(doc.get_element_by_id("main-nav"), Some(nav));
        assert_eq!(doc.get_element_by_id("missing"), None);
        assert_eq!(
            doc.get_elements_by_tag_name("P"),
            vec![p],
            "HTML tag lookup is case-insensitive"
        );
        assert_eq!(doc.get_elements_by_tag_name("nav"), vec![nav]);
        assert_eq!(
            doc.get_elements_by_tag_name("circle"),
            vec![svg],
            "foreign tags match exactly"
        );
        // `"*"` matches every element in any namespace, in document order (#688).
        assert_eq!(
            doc.get_elements_by_tag_name("*"),
            vec![html, head, body, nav, p, svg],
            "wildcard enumerates every element, HTML and foreign alike"
        );
        assert!(
            doc.get_elements_by_tag_name("nope*").is_empty(),
            "only the exact wildcard matches everything"
        );
        // Attribute names are ASCII case-insensitive on HTML elements for every operation
        // (#686): get/has/remove/set must agree, and set must not create duplicates.
        let nav_element = doc.node(nav).as_element().expect("nav element");
        assert_eq!(nav_element.get_attribute("ID"), Some("main-nav"));
        assert!(nav_element.has_attribute("Id"));
        assert_eq!(nav_element.get_attribute("class"), Some("top primary"));
        let mut probe = ElementData::new(Namespace::Html, "div");
        probe.attributes.push(Attribute::new("id", "lower"));
        // `set_attribute` returns true when it inserted, false when it overwrote.
        assert!(!probe.set_attribute(Attribute::new("id", "lower"), true));
        assert!(
            !probe.set_attribute(Attribute::new("ID", "upper"), true),
            "setAttribute with a different case must reuse the existing slot"
        );
        assert_eq!(
            probe.attribute_names(),
            vec!["id".to_string()],
            "setAttribute must not duplicate an attribute that differs only in case"
        );
        assert_eq!(probe.get_attribute("id"), Some("upper"));
        assert_eq!(probe.get_attribute("Id"), Some("upper"));
        assert_eq!(probe.remove_attribute("iD"), Some("upper".to_string()));
        assert_eq!(probe.get_attribute("id"), None);

        assert_eq!(doc.get_elements_by_class_name("primary"), vec![nav]);
        assert_eq!(doc.get_elements_by_class_name("top primary"), vec![nav]);
        assert!(doc.get_elements_by_class_name("top missing").is_empty());
        assert!(doc.get_elements_by_class_name("  ").is_empty());
    }

    #[test]
    fn set_attribute_overwrite_respects_value_byte_cap() {
        // Regression test for #1106: the overwrite path used by scripted setters
        // bypassed MAX_ATTRIBUTE_VALUE_BYTES.
        let mut element = ElementData::new(Namespace::Html, "div");
        assert!(element.set_attribute(Attribute::new("id", "x"), false));
        assert!(
            !element.set_attribute(Attribute::new("id", "y".repeat(1024 * 1024)), true),
            "an oversized overwrite must be rejected"
        );
        assert_eq!(element.get_attribute("id"), Some("x"));
        // A value within the cap still overwrites normally.
        let allowed = "y".repeat(MAX_ATTRIBUTE_VALUE_BYTES);
        assert!(!element.set_attribute(Attribute::new("id", allowed.clone()), true));
        assert_eq!(element.get_attribute("id"), Some(allowed.as_str()));
    }

    #[test]
    fn test_attribute_helpers_and_tag_name() {
        let mut doc = Document::new();
        let div = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "x"),
                Attribute::new("Class", "a"),
                Attribute::new("data-n", "1"),
            ],
        );
        let svg = doc.create_element(Namespace::Svg, "linearGradient", vec![]);

        {
            let element = doc.node(div).as_element().expect("div element");
            assert_eq!(element.tag_name(), "DIV");
            assert!(
                element.has_attribute("ID"),
                "HTML attribute names are case-insensitive"
            );
            assert!(element.has_attribute("class"));
            assert!(!element.has_attribute("missing"));
            assert_eq!(
                element.attribute_names(),
                vec!["id".to_string(), "Class".to_string(), "data-n".to_string()]
            );
            let foreign = doc.node(svg).as_element().expect("svg element");
            assert_eq!(foreign.tag_name(), "linearGradient");
            assert!(!foreign.has_attribute("lineargradient"));
        }
        let removed = doc
            .node_mut(div)
            .as_element_mut()
            .expect("div element")
            .remove_attribute("class");
        assert_eq!(removed.as_deref(), Some("a"));
        assert_eq!(
            doc.node(div)
                .as_element()
                .expect("div element")
                .attribute_names(),
            vec!["id".to_string(), "data-n".to_string()]
        );
    }

    #[test]
    fn test_text_content_and_set_text_content() {
        let mut doc = Document::new();
        let root = doc.root();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let text = doc.create_text_node("Hello ");
        let bold = doc.create_element(Namespace::Html, "b", vec![]);
        let bold_text = doc.create_text_node("world");
        doc.append_child(root, body);
        doc.append_child(body, div);
        doc.append_child(div, text);
        doc.append_child(div, bold);
        doc.append_child(bold, bold_text);
        let comment = doc.create_comment("ignored");
        doc.append_child(div, comment);

        assert_eq!(doc.text_content(div), "Hello world");
        assert_eq!(doc.text_content(text), "Hello ");
        assert_eq!(doc.text_content(doc.root()), "");

        doc.set_text_content(div, "replaced")
            .expect("set textContent");
        assert_eq!(doc.text_content(div), "replaced");
        assert_eq!(doc.children(div).count(), 1);
        // Removed children stay alive: DOM-style removal is detach-only.
        assert!(doc.owns(text));
        assert!(doc.owns(bold));
        doc.set_text_content(text, "updated")
            .expect("set text node data");
        assert_eq!(doc.node(text).as_text(), Some("updated"));
        assert!(matches!(
            doc.set_text_content(doc.root(), "x"),
            Err(DomError::CannotHaveChildren(_))
        ));
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn text_content_truncates_on_utf8_boundary() {
        // Regression test: a byte-count truncation could slice through a multi-byte
        // code point and panic (issue #1105).
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        let _ = doc.append_text(div, "a"); // shift `remaining` to an odd byte offset
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(div, span);
        let mut big = String::new();
        while big.len() <= Document::MAX_TEXT_CONTENT_BYTES {
            big.push('é');
        }
        let _ = doc.append_text(span, &big);

        // Before the fix this panicked with "end byte index ... is not a char boundary".
        let text = doc.text_content(div);
        assert!(text.len() <= Document::MAX_TEXT_CONTENT_BYTES);
        assert_eq!(&text[..1], "a");
        assert_eq!(text.chars().last(), Some('é'));
    }

    fn cyclic_text_children(doc: &mut Document, parent: NodeId) {
        let a = doc.append_text(parent, "a").expect("text a");
        let b = doc.append_text(parent, "b").expect("text b");
        let (ai, bi) = (a.index(), b.index());
        doc.nodes[ai].next_sibling = Some(b);
        doc.nodes[bi].next_sibling = Some(a);
    }

    #[test]
    fn text_content_returns_on_cyclic_sibling_chain() {
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        cyclic_text_children(&mut doc, div);
        // Before the fix this hung forever on the cyclic sibling chain (#1107).
        let _ = doc.text_content(div);
    }

    #[test]
    fn normalize_returns_on_cyclic_child_chain() {
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        cyclic_text_children(&mut doc, div);
        // Before the fix this hung forever (#1108).
        let _ = doc.normalize(div);
    }

    #[test]
    fn remove_node_returns_on_cyclic_child_chain() {
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        cyclic_text_children(&mut doc, div);
        // Before the fix this hung forever (#1109).
        assert!(doc.remove_node(div));
    }

    #[test]
    fn reparent_children_returns_on_cyclic_parent_chain() {
        let mut doc = Document::new();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        let c = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(doc.root(), a);
        doc.append_child(a, b);
        doc.append_child(doc.root(), c);
        let d = doc.create_text_node("d");
        doc.append_child(c, d);
        let (ai, bi) = (a.index(), b.index());
        doc.nodes[ai].parent = Some(b);
        doc.nodes[bi].parent = Some(a);
        // Before the fix is_ancestor_of(d, a) looped forever here (#1110).
        assert!(!doc.reparent_children(c, a));
    }

    #[test]
    fn shallow_clone_of_oversized_template_stays_bounded() {
        let mut doc = Document::new();
        let template = doc.create_element(Namespace::Html, "template", vec![]);
        doc.append_child(doc.root(), template);
        let contents = doc.template_contents_target(template);
        // Separate nodes: append_text() would coalesce them into a single text node.
        for _ in 0..=MAX_CLONE_NODES {
            let text = doc.create_text_node("x");
            doc.append_child(contents, text);
        }
        let clone = doc.clone_node(template, false).unwrap();
        let clone_contents = doc
            .node(clone)
            .as_element()
            .and_then(|el| el.template_contents)
            .expect("clone has its own template contents");
        // The oversized contents are skipped instead of cloned without a cap (#1111).
        assert_eq!(doc.node(clone_contents).first_child, None);
    }

    #[test]
    fn text_content_terminates_on_parent_child_and_self_template_cycles() {
        // Parent-child cycle must terminate cleanly.
        let mut doc = Document::new();
        let outer = doc.create_element(Namespace::Html, "div", vec![]);
        let inner = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(outer, inner);
        doc.nodes[inner.index()].first_child = Some(outer);
        let _ = doc.text_content(outer);

        // Self-referential `<template>` contents must not recurse forever on the call stack.
        let mut tmpl_doc = Document::new();
        let tmpl = tmpl_doc.create_element(Namespace::Html, "template", vec![]);
        if let Some(el) = tmpl_doc.node_mut(tmpl).as_element_mut() {
            el.template_contents = Some(tmpl);
        }
        let _ = tmpl_doc.text_content(tmpl);
    }

    #[test]
    fn normalize_rejects_cycles_without_mutating_corrupted_siblings() {
        let mut doc = Document::new();
        let container = doc.create_element(Namespace::Html, "div", vec![]);
        let t1 = doc.create_text_node("a");
        let t2 = doc.create_text_node("b");
        doc.append_child(container, t1);
        doc.append_child(container, t2);
        doc.nodes[t2.index()].next_sibling = Some(t1);
        assert_eq!(doc.normalize(container), 0);
        assert_eq!(doc.node(t1).as_text(), Some("a"));

        // Parent-child cycle must also terminate cleanly.
        let mut cycle_doc = Document::new();
        let outer = cycle_doc.create_element(Namespace::Html, "div", vec![]);
        let inner = cycle_doc.create_element(Namespace::Html, "span", vec![]);
        cycle_doc.append_child(outer, inner);
        cycle_doc.nodes[inner.index()].first_child = Some(outer);
        assert_eq!(cycle_doc.normalize(outer), 0);
    }

    #[test]
    fn remove_node_terminates_on_cyclic_child_chain_with_following_sibling() {
        for with_following_sibling in [false, true] {
            let mut doc = Document::new();
            let root = doc.root();
            let wrapper = doc.create_element(Namespace::Html, "div", vec![]);
            let sibling = doc.create_element(Namespace::Html, "p", vec![]);
            let a = doc.create_element(Namespace::Html, "span", vec![]);
            let b = doc.create_element(Namespace::Html, "em", vec![]);
            doc.append_child(root, wrapper);
            if with_following_sibling {
                doc.append_child(root, sibling);
            }
            doc.append_child(wrapper, a);
            doc.append_child(wrapper, b);
            doc.nodes[b.index()].next_sibling = Some(a);
            doc.nodes[a.index()].prev_sibling = Some(b);

            assert!(doc.remove_node(wrapper));
            assert!(doc.try_node(wrapper).is_none());
        }
    }

    #[test]
    fn compare_document_position_terminates_on_cyclic_siblings() {
        let mut doc = Document::new();
        let common = doc.create_element(Namespace::Html, "div", vec![]);
        let c1 = doc.create_element(Namespace::Html, "span", vec![]);
        let c2 = doc.create_element(Namespace::Html, "em", vec![]);
        let orphan_a = doc.create_element(Namespace::Html, "a", vec![]);
        let orphan_b = doc.create_element(Namespace::Html, "b", vec![]);
        doc.append_child(doc.root(), common);
        doc.append_child(common, c1);
        doc.append_child(common, c2);
        // Point orphan_a/orphan_b parent to common without linking them into c1 -> c2 chain,
        // and turn c1 <-> c2 into a sibling cycle.
        doc.nodes[orphan_a.index()].parent = Some(common);
        doc.nodes[orphan_b.index()].parent = Some(common);
        doc.nodes[c2.index()].next_sibling = Some(c1);

        let _ = doc.compare_document_position(orphan_a, orphan_b);
    }

    #[test]
    fn clone_node_invalid_handle_returns_none() {
        let mut doc = Document::new();
        let victim = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), victim);
        assert!(doc.remove_node(victim));
        assert_eq!(doc.clone_node(victim, true), None);
        assert_eq!(doc.clone_node(victim, false), None);
    }

    #[test]
    fn test_template_text_content_routes_to_contents_fragment() {
        let mut doc = Document::new();
        let template = doc.create_element(Namespace::Html, "template", vec![]);
        let fragment = doc
            .node(template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("template contents");
        doc.set_text_content(template, "inert")
            .expect("set template text");
        assert_eq!(doc.text_content(template), "inert");
        assert_eq!(doc.children(fragment).count(), 1);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_remove_insert_and_replace_children() {
        let mut doc = Document::new();
        let root = doc.root();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let a = doc.create_element(Namespace::Html, "span", vec![]);
        let b = doc.create_element(Namespace::Html, "em", vec![]);
        let c = doc.create_element(Namespace::Html, "i", vec![]);
        doc.append_child(root, div);
        doc.append_child(div, a);
        doc.append_child(div, b);

        // insertBefore with a valid reference.
        doc.insert_before_child(div, c, Some(b))
            .expect("insert before b");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![a, c, b]);
        // Appending moves the node.
        doc.insert_before_child(div, a, None).expect("append a");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![c, b, a]);
        // Invalid reference is rejected.
        assert!(matches!(
            doc.insert_before_child(div, c, Some(div)),
            Err(DomError::NotAChild { .. })
        ));
        // removeChild detaches but keeps the node alive.
        assert_eq!(doc.remove_child(div, c), Ok(c));
        assert_eq!(doc.node(c).parent, None);
        assert!(doc.owns(c));
        assert!(matches!(
            doc.remove_child(div, c),
            Err(DomError::NotAChild { .. })
        ));
        // replaceChildren detaches the old children and keeps them alive.
        let d = doc.create_element(Namespace::Html, "u", vec![]);
        doc.replace_all_children(div, &[d])
            .expect("replace children");
        assert_eq!(doc.children(div).collect::<Vec<_>>(), vec![d]);
        assert!(doc.owns(a) && doc.owns(b));
        assert_eq!(doc.node(a).parent, None);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn test_append_text_node_coalesces() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let first = doc.append_text_node(body, "Hello").expect("append text");
        let second = doc.append_text_node(body, " world").expect("append text");
        assert_eq!(first, second, "trailing text nodes coalesce");
        assert_eq!(doc.children(body).count(), 1);
        assert_eq!(doc.text_content(body), "Hello world");
    }

    #[test]
    fn test_clone_node_into_another_document() {
        let mut source = Document::new();
        let source_body = source.create_element(Namespace::Html, "body", vec![]);
        source.append_child(source.root(), source_body);
        let source_div =
            source.create_element(Namespace::Html, "div", vec![Attribute::new("id", "cloned")]);
        let template = source.create_element(Namespace::Html, "template", vec![]);
        source.append_child(source_body, source_div);
        source.append_child(source_div, template);
        source
            .set_text_content(template, "inert text")
            .expect("template text");

        let mut target = Document::new();
        let imported = target
            .clone_node_into(&source, source_div, true)
            .expect("import subtree");
        assert!(target.owns(imported));
        assert_ne!(imported, source_div);
        assert_eq!(
            target
                .node(imported)
                .as_element()
                .unwrap()
                .get_attribute("id"),
            Some("cloned")
        );
        target.append_child(target.root(), imported);
        // DOM tree traversal does not descend into inert <template> contents.
        assert_eq!(target.text_content(imported), "");
        let imported_template = target
            .children(imported)
            .find(|&id| {
                target
                    .node(id)
                    .as_element()
                    .is_some_and(|el| el.local_name == "template")
            })
            .expect("imported template");
        let imported_contents = target
            .node(imported_template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("fresh contents fragment");
        assert!(target.owns(imported_contents));
        assert!(!source.owns(imported_contents));
        assert_eq!(target.children(imported_contents).count(), 1);
        assert_eq!(target.text_content(imported_template), "inert text");
        assert!(target.validate_invariants().is_ok());
        assert!(source.validate_invariants().is_ok());
        // The source tree is untouched.
        assert_eq!(source.text_content(source_div), "");
        let source_template = source
            .children(source_div)
            .find(|&id| {
                source
                    .node(id)
                    .as_element()
                    .is_some_and(|el| el.local_name == "template")
            })
            .expect("source template");
        assert_eq!(source.text_content(source_template), "inert text");
    }

    #[test]
    fn test_browser_api_respects_stale_handles() {
        let mut doc = Document::new();
        let root = doc.root();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(root, html);
        doc.append_child(html, body);
        let victim = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(body, victim);
        let removed = doc.remove_subtree(victim);
        assert_eq!(removed, 1);
        assert!(!doc.owns(victim));
        assert!(matches!(
            doc.remove_child(body, victim),
            Err(DomError::UnknownNode(_))
        ));
        assert!(matches!(
            doc.insert_before_child(body, victim, None),
            Err(DomError::UnknownNode(_))
        ));
        // Lookups keep working after removals.
        assert_eq!(doc.body(), Some(body));
        assert_eq!(doc.get_elements_by_tag_name("div"), Vec::<NodeId>::new());
        assert!(doc.validate_invariants().is_ok());
    }
}

#[cfg(test)]
mod fragment_insertion_tests {
    use crate::*;

    fn container_children(document: &Document, parent: NodeId) -> Vec<String> {
        document
            .children(parent)
            .filter_map(|child| match &document.node(child).kind {
                NodeKind::Element(element) => Some(element.local_name.clone()),
                NodeKind::Text(text) => Some(format!("#text:{text}")),
                _ => None,
            })
            .collect()
    }

    fn setup() -> (Document, NodeId, NodeId) {
        let mut document = Document::new();
        let root = document.root();
        let html = document.create_element(Namespace::Html, "html", Vec::new());
        document.append_child(root, html);
        let body = document.create_element(Namespace::Html, "body", Vec::new());
        document.append_child(html, body);
        (document, body, html)
    }

    fn element(document: &mut Document, name: &str) -> NodeId {
        document.create_element(Namespace::Html, name, Vec::new())
    }

    #[test]
    fn append_child_with_fragment_expands_children_in_order() {
        let (mut document, body, _html) = setup();
        let fragment = document.create_document_fragment();
        let a = element(&mut document, "a");
        let b = element(&mut document, "b");
        document.append_child(fragment, a);
        document.append_child(fragment, b);
        let inserted = document.insert_node(body, fragment, None).expect("insert");
        assert_eq!(inserted, vec![a, b]);
        assert_eq!(container_children(&document, body), vec!["a", "b"]);
        assert_eq!(
            container_children(&document, fragment),
            Vec::<String>::new()
        );
    }

    #[test]
    fn insert_before_with_fragment_keeps_reference_position() {
        let (mut document, body, _html) = setup();
        let first = element(&mut document, "first");
        let last = element(&mut document, "last");
        document.append_child(body, first);
        document.append_child(body, last);
        let fragment = document.create_document_fragment();
        let x = element(&mut document, "x");
        let y = element(&mut document, "y");
        document.append_child(fragment, x);
        document.append_child(fragment, y);
        document
            .insert_node(body, fragment, Some(last))
            .expect("insert fragment");
        assert_eq!(
            container_children(&document, body),
            vec!["first", "x", "y", "last"]
        );
    }

    #[test]
    fn replace_node_keeps_position_and_expands_fragments() {
        let (mut document, body, _html) = setup();
        let a = element(&mut document, "a");
        let b = element(&mut document, "b");
        let c = element(&mut document, "c");
        for node in [a, b, c] {
            document.append_child(body, node);
        }
        let fragment = document.create_document_fragment();
        let x = element(&mut document, "x");
        let y = element(&mut document, "y");
        document.append_child(fragment, x);
        document.append_child(fragment, y);
        document.replace_node(body, fragment, b).expect("replace");
        assert_eq!(
            container_children(&document, body),
            vec!["a", "x", "y", "c"]
        );
    }

    #[test]
    fn insert_node_accepts_a_document_parent() {
        let (mut document, _body, html) = setup();
        let body = document.body().expect("body");
        document.remove_child(html, body).expect("detach body");
        document
            .remove_child(document.root(), html)
            .expect("detach html");
        document
            .insert_node(document.root(), html, None)
            .expect("document.appendChild");
        assert_eq!(
            document.children(document.root()).collect::<Vec<_>>(),
            vec![html]
        );
        assert_eq!(document.document_element(), Some(html));
    }

    #[test]
    fn replace_node_rejects_non_children() {
        let (mut document, body, _html) = setup();
        let orphan = element(&mut document, "orphan");
        let new_node = element(&mut document, "new");
        assert!(matches!(
            document.replace_node(body, new_node, orphan),
            Err(DomError::NotAChild { .. })
        ));
    }
    #[test]
    fn insert_before_moving_the_previous_sibling_keeps_the_chain() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let a = doc.create_element(Namespace::Html, "a", vec![]);
        let b = doc.create_element(Namespace::Html, "b", vec![]);
        doc.append_child(body, a);
        doc.append_child(body, b);

        assert!(doc.insert_before(a, b));
        let children: Vec<NodeId> = doc.children(body).collect();
        assert_eq!(children, vec![b, a], "no cycle and both children present");
        assert!(doc.validate_invariants().is_ok());

        assert!(doc.insert_before(b, a));
        let children: Vec<NodeId> = doc.children(body).collect();
        assert_eq!(children, vec![a, b]);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn the_document_node_cannot_become_a_child_or_be_freed() {
        let mut doc = Document::new();
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), div);
        assert!(!doc.append_child(div, doc.root()), "document as a child");
        assert_eq!(doc.remove_subtree(doc.root()), 0, "the root is never freed");
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn normalize_handles_a_very_deep_chain_iteratively() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let mut parent = body;
        for _ in 0..50_000 {
            let child = doc.create_element(Namespace::Html, "div", vec![]);
            doc.append_child(parent, child);
            parent = child;
        }
        let text = doc.create_text_node("hello");
        doc.append_child(parent, text);
        let removed = doc.normalize(body);
        assert_eq!(removed, 0);
        assert!(doc.validate_invariants().is_ok());
    }

    #[test]
    fn normalize_keeps_removed_text_nodes_alive() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let paragraph = doc.create_element(Namespace::Html, "p", vec![]);
        doc.append_child(body, paragraph);
        let first = doc.create_text_node("a");
        let second = doc.create_text_node("b");
        doc.append_child(paragraph, first);
        doc.append_child(paragraph, second);
        assert_eq!(doc.normalize(paragraph), 1, "one text node merged");
        assert_eq!(doc.node(second).as_text(), Some("b"));
        assert_eq!(doc.node(second).parent, None);
        assert_eq!(doc.node(first).as_text(), Some("ab"));
        assert!(doc.validate_invariants().is_ok());
    }
}

#[cfg(test)]
mod template_child_edges {
    use crate::*;

    #[test]
    fn template_child_nodes_stay_empty_and_content_is_separate() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        doc.append_child(doc.root(), body);
        let template = doc.create_element(Namespace::Html, "template", vec![]);
        doc.append_child(body, template);
        let fragment = doc
            .node(template)
            .as_element()
            .and_then(|element| element.template_contents)
            .expect("template contents");
        let span = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(fragment, span);
        assert_eq!(
            doc.children(template).count(),
            0,
            "template.childNodes is empty"
        );
        assert_eq!(
            doc.children(fragment).count(),
            1,
            "content lives in the contents fragment"
        );
        assert_eq!(doc.node(span).parent, Some(fragment));
    }
}

