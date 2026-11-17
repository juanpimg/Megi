//! Layout box-tree, clearfix, attr(), CSS counter, and inline wrapping tests for `::before` / `::after` (#1540).

use css::{MediaContext, resolve_document_styles_with_pseudos};
use dom::{Attribute, Document, Namespace, NodeId};
use layout::{
    BoxType, LayoutBox, Rect, build_layout_tree_with_pseudos, layout_document_with_pseudos,
};

fn elem(doc: &mut Document, parent: NodeId, tag: &str, attrs: Vec<(&str, &str)>) -> NodeId {
    let attrs = attrs
        .into_iter()
        .map(|(k, v)| Attribute {
            namespace: Namespace::None,
            prefix: None,
            local_name: k.to_string(),
            value: v.to_string(),
        })
        .collect();
    let id = doc.create_element(Namespace::Html, tag, attrs);
    doc.append_child(parent, id);
    id
}

fn text_node(doc: &mut Document, parent: NodeId, data: &str) -> NodeId {
    let id = doc.create_text_node(data);
    doc.append_child(parent, id);
    id
}

fn collect_texts(node: &LayoutBox, out: &mut Vec<String>) {
    if let BoxType::TextNode(text) = &node.box_type {
        out.push(text.clone());
    }
    for child in &node.children {
        collect_texts(child, out);
    }
}

fn find_first_box(node: &LayoutBox, node_id: NodeId, is_pseudo: bool) -> Option<&LayoutBox> {
    if node.node_id() == Some(node_id) && node.is_pseudo == is_pseudo {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_first_box(child, node_id, is_pseudo) {
            return Some(found);
        }
    }
    None
}

fn count_pseudo_inline_boxes(node: &LayoutBox, node_id: NodeId) -> usize {
    let mut count = usize::from(
        node.node_id() == Some(node_id)
            && node.is_pseudo
            && matches!(node.box_type, BoxType::InlineNode(_)),
    );
    for child in &node.children {
        count += count_pseudo_inline_boxes(child, node_id);
    }
    count
}

#[test]
fn before_and_after_boxes_insert_in_order_and_ignore_replaced_elements() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", vec![]);
    let body = elem(&mut doc, html, "body", vec![]);
    let p = elem(&mut doc, body, "p", vec![]);
    text_node(&mut doc, p, "middle");
    elem(&mut doc, body, "img", vec![("src", "a.png")]);

    let style_text = "p::before { content: \"START:\"; } p::after { content: \":END\"; } img::before { content: \"SHOULD NOT EXIST\"; }";
    let (styles, pseudos) =
        resolve_document_styles_with_pseudos(&doc, Some(style_text), MediaContext::default());

    let tree = build_layout_tree_with_pseudos(&doc, &styles, &pseudos, None).expect("layout tree");
    let mut texts = Vec::new();
    collect_texts(&tree, &mut texts);
    assert_eq!(texts, vec!["START:", "middle", ":END"]);

    let p_box = find_first_box(&tree, p, false).expect("p box");
    assert_eq!(p_box.children.len(), 3);
    assert!(p_box.children[0].is_pseudo);
    assert!(!p_box.children[1].is_pseudo);
    assert!(p_box.children[2].is_pseudo);
}

#[test]
fn clearfix_after_pseudo_element_expands_parent_height_around_floats() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", vec![]);
    let body = elem(&mut doc, html, "body", vec![]);
    let row = elem(&mut doc, body, "div", vec![("class", "row")]);
    elem(&mut doc, row, "div", vec![("class", "fl")]);

    let css_src = r#"
        body { margin: 0; }
        .fl { float: left; width: 50px; height: 80px; }
        .row::after { content: ""; display: block; clear: both; }
    "#;
    let (styles, pseudos) = resolve_document_styles_with_pseudos(
        &doc,
        Some(css_src),
        MediaContext::with_viewport(400.0, 300.0),
    );
    let root_box =
        layout_document_with_pseudos(&doc, &styles, &pseudos, Rect::new(0.0, 0.0, 400.0, 300.0))
            .expect("laid out");

    let row_box = find_first_box(&root_box, row, false).expect("row box");
    assert!(
        row_box.dimensions.content.height >= 80.0,
        "clearfix ::after must clear the 80px float, got height {}",
        row_box.dimensions.content.height
    );
}

#[test]
fn counters_and_attr_resolve_across_nested_lists() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", vec![]);
    let body = elem(&mut doc, html, "body", vec![]);
    let ol1 = elem(&mut doc, body, "ol", vec![]);
    let li1 = elem(&mut doc, ol1, "li", vec![]);
    text_node(&mut doc, li1, "A");
    let ol2 = elem(&mut doc, li1, "ol", vec![]);
    let li1a = elem(&mut doc, ol2, "li", vec![]);
    text_node(&mut doc, li1a, "A1");
    let li1b = elem(&mut doc, ol2, "li", vec![]);
    text_node(&mut doc, li1b, "A2");
    let li2 = elem(&mut doc, ol1, "li", vec![("data-note", "!")]);
    text_node(&mut doc, li2, "B");

    let css_src = r#"
        ol { counter-reset: item; }
        li { counter-increment: item; }
        li::before { content: counter(item) ". "; }
        li[data-note]::after { content: attr(data-note); }
    "#;
    let (styles, pseudos) =
        resolve_document_styles_with_pseudos(&doc, Some(css_src), MediaContext::default());
    let tree = build_layout_tree_with_pseudos(&doc, &styles, &pseudos, None).expect("tree");

    let mut texts = Vec::new();
    collect_texts(&tree, &mut texts);
    assert_eq!(
        texts,
        vec!["1. ", "A", "1. ", "A1", "2. ", "A2", "2. ", "B", "!"]
    );
}

#[test]
fn inline_pseudo_element_split_across_lines_preserves_is_pseudo() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html", vec![]);
    let body = elem(&mut doc, html, "body", vec![]);
    let p = elem(&mut doc, body, "p", vec![]);
    text_node(&mut doc, p, "tail");

    let css_src = r#"
        body { margin: 0; }
        p { width: 60px; font-size: 16px; }
        p::before { content: "one two three four"; }
    "#;
    let (styles, pseudos) = resolve_document_styles_with_pseudos(
        &doc,
        Some(css_src),
        MediaContext::with_viewport(200.0, 200.0),
    );
    let laid_out =
        layout_document_with_pseudos(&doc, &styles, &pseudos, Rect::new(0.0, 0.0, 200.0, 200.0))
            .expect("laid out");

    let split_count = count_pseudo_inline_boxes(&laid_out, p);
    assert!(
        split_count >= 2,
        "wrapping inline ::before must produce >=2 InlineBox fragments all marked is_pseudo, got {split_count}"
    );
}
