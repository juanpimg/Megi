pub(crate) use std::collections::HashMap;

pub(crate) use megi_core::{Document, Namespace, NodeId};

pub(crate) use megi_css::{ComputedStyle, Display, Length, VerticalAlign, WhiteSpace};

pub(crate) use megi_layout::{BoxType, Rect, layout_document};

pub(crate) fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Block;
    s.width = width;
    s.height = height;
    s
}

pub(crate) fn make_inline_style() -> ComputedStyle {
    let mut s = ComputedStyle::initial();
    s.display = Display::Inline;
    s
}

pub(crate) fn setup_simple_doc(
    text: &str,
    container_style: ComputedStyle,
) -> (Document, HashMap<NodeId, ComputedStyle>) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let p = doc.create_element(Namespace::Html, "p", vec![]);

    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, p);
    doc.append_text(p, text);

    let mut styles = HashMap::new();
    styles.insert(html, make_block_style(Length::Auto, Length::Auto));
    styles.insert(body, make_block_style(Length::Auto, Length::Auto));
    styles.insert(p, container_style);

    (doc, styles)
}
