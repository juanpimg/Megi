//! The in-place fragment parser must never lose the caller's document.
//!
//! Two audit findings motivated these tests:
//!
//! * #1444 — the 256-byte cap on the fragment context name sliced `&trimmed[..256]`
//!   without checking a code-point boundary, so a multibyte context name panicked.
//! * #1454 — that panic used to be caught by `parse_fragment_into`, which then replaced
//!   the caller's `Document` with an empty one and returned `Ok`, so an `innerHTML`
//!   assignment could silently wipe the page.

use html::{Document, Namespace, ParseOptions, parse_fragment_into};

fn document_with_marker() -> (Document, html::NodeId) {
    let mut doc = Document::new();
    let marker = doc.create_element(Namespace::Html, "p", vec![]);
    doc.append_child(doc.root(), marker);
    (doc, marker)
}

#[test]
fn multibyte_context_name_over_the_cap_does_not_panic() {
    let (mut doc, marker) = document_with_marker();
    // 100 x U+5B57 is 300 bytes, so the 256-byte cap lands inside a three-byte code point.
    let context = "字".repeat(100);
    let before = doc.children_ids(doc.root()).count();

    let violations = parse_fragment_into(
        &mut doc,
        "<b>fragment</b>",
        Namespace::Html,
        &context,
        true,
        &ParseOptions::default(),
    );

    assert!(
        !violations.panicked,
        "a multibyte context name must not reach the panic path"
    );
    assert!(
        doc.children_ids(doc.root()).count() > before,
        "the fragment children were not appended"
    );
    assert!(
        doc.children_ids(doc.root())
            .any(|id| doc.text_content(id).contains("fragment")),
        "the parsed text is missing from the document"
    );
    assert!(
        doc.children_ids(doc.root()).any(|id| id == marker),
        "the caller's existing nodes must survive the fragment parse"
    );
}

#[test]
fn a_legal_fragment_parse_reports_no_violation() {
    let (mut doc, _marker) = document_with_marker();
    let violations = parse_fragment_into(
        &mut doc,
        "<i>ok</i>",
        Namespace::Html,
        "div",
        true,
        &ParseOptions::default(),
    );

    assert!(!violations.panicked);
    assert!(
        violations.is_empty(),
        "a legal fragment must not report limits: {violations:?}"
    );
}
