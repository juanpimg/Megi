//! CSS 2D transform layout application (#1539): `translate`/`scale`/`rotate` move the
//! painted box geometry through the transform's axis-aligned bounding box.

use megi_core::{Document, Namespace};
use megi_css::resolve_document_styles;
use megi_layout::{Rect, layout_document};

fn layout_for_style(css_text: &str) -> megi_layout::LayoutBox {
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
fn div_content(tree: &megi_layout::LayoutBox) -> Rect {
    fn deepest(node: &megi_layout::LayoutBox, depth: usize) -> (usize, Rect) {
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

// #1934: the pivot is the element's border box, not the page origin.
#[test]
fn transform_origin_controls_the_rotation_pivot() {
    let tree = layout_for_style(
        "body { margin:0 } div { width:60px; height:20px; margin-left:100px; \
         transform-origin:0 0; transform: rotate(90deg); }",
    );
    let rect = div_content(&tree);
    // rotate(90deg) about the border-box top-left (100,0) yields AABB (80,0,20,60).
    assert!((rect.x - 80.0).abs() < 0.5, "x={}", rect.x);
    assert!(rect.y.abs() < 0.5, "y={}", rect.y);
    assert!((rect.width - 20.0).abs() < 0.5, "width={}", rect.width);
    assert!((rect.height - 60.0).abs() < 0.5, "height={}", rect.height);
}

#[test]
fn default_transform_origin_is_the_box_center() {
    let tree = layout_for_style(
        "body { margin:0 } div { width:60px; height:20px; margin-left:100px; \
         transform: rotate(90deg); }",
    );
    let rect = div_content(&tree);
    // 50% 50% = (130,10): the rotated AABB is (120,-20,20,60).
    assert!((rect.x - 120.0).abs() < 0.5, "x={}", rect.x);
    assert!((rect.y + 20.0).abs() < 0.5, "y={}", rect.y);
}
