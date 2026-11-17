//! CSS 2D transform layout application (#1539): `translate`/`scale`/`rotate` move the
//! painted box geometry through the transform's axis-aligned bounding box.

use css::resolve_document_styles;
use dom::{Document, Namespace};
use layout::{Rect, layout_document};

fn layout_for_style(css_text: &str) -> layout::LayoutBox {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    doc.append_child(doc.root(), html);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(html, body);
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(body, div);
    let styles = resolve_document_styles(&doc, Some(css_text));
    layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0)).expect("layout succeeds")
}

/// The div is the deepest block box (grandchild of the layout root).
fn div_content(tree: &layout::LayoutBox) -> Rect {
    fn deepest(node: &layout::LayoutBox, depth: usize) -> (usize, Rect) {
        let mut best = (depth, node.dimensions.content);
        for child in &node.children {
            let candidate = deepest(child, depth + 1);
            if candidate.0 > best.0 {
                best = candidate;
            }
        }
        best
    }
    deepest(tree, 0).1
}

#[test]
fn translate_moves_the_box() {
    let tree = layout_for_style(
        "body { margin:0 } div { width:100px; height:50px; transform: translate(50px, 20px); }",
    );
    let rect = div_content(&tree);
    assert!((rect.x - 50.0).abs() < 0.5, "x={}", rect.x);
    assert!((rect.y - 20.0).abs() < 0.5, "y={}", rect.y);
}

#[test]
fn no_transform_keeps_normal_flow() {
    let tree = layout_for_style("body { margin:0 } div { width:100px; height:50px; }");
    let rect = div_content(&tree);
    assert!(rect.x.abs() < 0.5, "x={}", rect.x);
}
