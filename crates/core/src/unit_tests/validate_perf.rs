//! Regression tests for the linear reverse-membership validation (#1731).

use crate::*;

#[test]
fn validate_invariants_detects_orphan_parent_claim() {
    // #1731: the reverse-membership check (a node claiming a parent it is not in the child
    // chain of) must survive the O(n) refactor. Corrupt the arena the way a broken import
    // would: b claims `root` as parent but only `a` is in root's chain.
    let mut doc = Document::new();
    let root = doc.root();
    let a = doc.create_element(Namespace::Html, "a", vec![]);
    let b = doc.create_element(Namespace::Html, "b", vec![]);
    doc.append_child(root, a);
    doc.append_child(a, b);
    assert!(doc.validate_invariants().is_ok());

    // Detach b from a's chain, then make it claim root as its parent.
    doc.nodes[a.index()].first_child = None;
    doc.nodes[a.index()].last_child = None;
    doc.nodes[b.index()].prev_sibling = None;
    doc.nodes[b.index()].next_sibling = None;
    doc.nodes[b.index()].parent = Some(root);
    let error = doc
        .validate_invariants()
        .expect_err("orphan parent claim must be detected");
    assert!(
        error.contains("claims parent") || error.contains("child chain"),
        "unexpected error: {error}"
    );
}

#[test]
fn validate_invariants_wide_tree_is_linear() {
    // #1731: a wide star used to be O(n²) in reverse-membership validation. 20k children
    // with the linear implementation completes quickly; keep it as a regression exercise.
    let mut doc = Document::new();
    let root = doc.root();
    for _ in 0..20_000 {
        let child = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(root, child);
    }
    assert!(doc.validate_invariants().is_ok());
}
