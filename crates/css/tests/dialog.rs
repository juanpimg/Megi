//! Dialog UA styling (#1546): a `<dialog>` without `open` is `display: none`.

use css::{Display, resolve_document_styles};
use megi_core::{Attribute, Document, Namespace};

fn dialog_display(open: bool) -> Display {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let dialog = if open {
        doc.create_element(Namespace::Html, "dialog", vec![Attribute::new("open", "")])
    } else {
        doc.create_element(Namespace::Html, "dialog", vec![])
    };
    doc.append_child(html, dialog);
    let styles = resolve_document_styles(&doc, None);
    styles.get(&dialog).expect("dialog style").display
}

#[test]
fn closed_dialog_is_display_none() {
    assert_eq!(
        dialog_display(false),
        Display::None,
        "a dialog without `open` must not render"
    );
}

#[test]
fn open_dialog_is_not_hidden_by_ua_rule() {
    assert_ne!(
        dialog_display(true),
        Display::None,
        "an open dialog must not be hidden by the UA rule"
    );
}
