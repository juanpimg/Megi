//! Layout tests for `<dialog>`, `fit-content` + `margin: auto` centering, and top-layer clip escaping.

use megi_css::resolve_document_styles_with_viewport;
use megi_core::{Attribute, Document, Namespace, NodeId};
use megi_layout::{LayoutBox, Rect, layout_document};

fn elem(doc: &mut Document, parent: NodeId, tag: &str, attrs: &[(&str, &str)]) -> NodeId {
    let attrs = attrs
        .iter()
        .map(|(name, value)| Attribute::new(*name, *value))
        .collect();
    let id = doc.create_element(Namespace::Html, tag, attrs);
    doc.append_child(parent, id);
    id
}

fn find_box(root: &LayoutBox, target: NodeId) -> Option<&LayoutBox> {
    if root.node_id() == Some(target) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_box(child, target) {
            return Some(found);
        }
    }
    None
}

#[test]
fn closed_dialog_generates_no_layout_box() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", &[]);
    let body = elem(&mut doc, html, "body", &[]);
    let dialog = elem(&mut doc, body, "dialog", &[]);
    let p = elem(&mut doc, dialog, "p", &[]);
    let text = doc.create_text_node("Secret");
    doc.append_child(p, text);

    let styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("tree");
    assert!(find_box(&tree, dialog).is_none());
}

#[test]
fn modal_dialog_centers_in_viewport_and_escapes_ancestor_overflow_clip() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", &[]);
    let body = elem(&mut doc, html, "body", &[]);
    let clip_div = elem(
        &mut doc,
        body,
        "div",
        &[("style", "width:120px;height:80px;overflow:hidden")],
    );
    let dialog = elem(
        &mut doc,
        clip_div,
        "dialog",
        &[
            ("open", ""),
            ("data-megi-modal", "1"),
            ("style", "width:200px;height:100px;padding:0;border-width:0"),
        ],
    );
    let text = doc.create_text_node("Hi");
    doc.append_child(dialog, text);

    let styles = resolve_document_styles_with_viewport(&doc, None, 800.0, 600.0);
    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("tree");

    let clip_box = find_box(&tree, clip_div).expect("clip box");
    assert!(clip_box.clip.is_some());

    let dlg_box = find_box(&tree, dialog).expect("dialog box");
    assert!(dlg_box.is_top_layer);
    assert_eq!(dlg_box.top_layer_order, 1);
    assert!(dlg_box.establishes_stacking_context);
    assert!(
        dlg_box.clip.is_none() || dlg_box.clip.unwrap().width >= 200.0,
        "modal dialog must escape 120px ancestor overflow clip, got {:?}",
        dlg_box.clip
    );

    let border_box = dlg_box.dimensions.border_box();
    assert!((border_box.width - 200.0).abs() < 1.0, "{border_box:?}");
    assert!((border_box.height - 100.0).abs() < 1.0, "{border_box:?}");
    assert!(
        (border_box.x - 300.0).abs() < 1.0,
        "expected x=300, got {border_box:?}"
    );
    assert!(
        (border_box.y - 250.0).abs() < 1.0,
        "expected y=250, got {border_box:?}"
    );
}
