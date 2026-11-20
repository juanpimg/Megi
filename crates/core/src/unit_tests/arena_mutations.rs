//! Corrupt-arena guards, cloning and browser-style child mutation tests.

use crate::*;

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

#[test]
fn remove_subtree_terminates_on_cyclic_siblings() {
    // #1730: a corrupt arena with a cyclic sibling chain must not hang or grow `pending`
    // without bound. Free the outer subtree; the walk must be capped at the arena size.
    let mut doc = Document::new();
    let root = doc.root();
    let parent = doc.create_element(Namespace::Html, "div", vec![]);
    let a = doc.create_element(Namespace::Html, "span", vec![]);
    let b = doc.create_element(Namespace::Html, "em", vec![]);
    doc.append_child(root, parent);
    doc.append_child(parent, a);
    doc.append_child(parent, b);
    // Make b's next_sibling point back to a (cycle).
    doc.nodes[b.index()].next_sibling = Some(a);
    // Must terminate and free at most the number of live nodes.
    let freed = doc.remove_subtree(parent);
    assert!((1..=4).contains(&freed), "freed {freed}");
    assert!(doc.try_node(parent).is_none());
}

#[test]
fn clone_node_into_terminates_on_cyclic_siblings_in_source() {
    // #1730: import_shallow walks the source sibling chain with no cycle guard; a corrupt
    // source arena must be rejected (NodeLimitExceeded), not loop forever.
    let mut source = Document::new();
    let parent = source.create_element(Namespace::Html, "div", vec![]);
    let a = source.create_element(Namespace::Html, "span", vec![]);
    let b = source.create_element(Namespace::Html, "em", vec![]);
    source.append_child(source.root(), parent);
    source.append_child(parent, a);
    source.append_child(parent, b);
    source.nodes[b.index()].next_sibling = Some(a);

    let mut dest = Document::new();
    let result = dest.clone_node_into(&source, parent, true);
    // Either an error or a bounded clone is acceptable — the key is termination.
    if let Ok(id) = result {
        assert!(dest.subtree_size(id) <= MAX_CLONE_NODES);
    }
}
