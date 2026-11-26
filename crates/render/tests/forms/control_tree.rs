//! Layout-tree contract for generated native-control text.
//!
//! Replaces the early `diag.rs` print-only probe with assertions that keep the
//! select path and unsupported input types from regressing when the input path
//! started generating text children.

use megi_layout::{BoxType, LayoutBox};
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

fn html_data_url(html: &str) -> String {
    let mut encoded = String::with_capacity(html.len());
    for byte in html.bytes() {
        match byte {
            b'#' => encoded.push_str("%23"),
            b'%' => encoded.push_str("%25"),
            b'?' => encoded.push_str("%3F"),
            b'\n' => encoded.push_str("%0A"),
            b'\r' => encoded.push_str("%0D"),
            b' ' => encoded.push_str("%20"),
            _ if byte.is_ascii() => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("data:text/html;charset=utf-8,{encoded}")
}

fn render(body: &str) -> RenderOutput {
    let html = format!("<!doctype html><body style='margin:0'>{body}</body>");
    render_url(
        &html_data_url(&html),
        &RenderOptions {
            viewport: Viewport::new(640, 200).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

fn replaced_for_tag<'a>(
    node: &'a LayoutBox,
    tag: &str,
    out: &RenderOutput,
) -> Option<&'a LayoutBox> {
    match &node.box_type {
        BoxType::Replaced(id)
            if out
                .document
                .node(*id)
                .as_element()
                .is_some_and(|el| el.local_name == tag) =>
        {
            return Some(node);
        }
        _ => {}
    }
    node.children
        .iter()
        .find_map(|child| replaced_for_tag(child, tag, out))
}

fn generated_text(out: &RenderOutput, tag: &str) -> Option<String> {
    let box_ = replaced_for_tag(&out.layout, tag, out)?;
    match box_.children.first().map(|child| &child.box_type) {
        Some(BoxType::TextNode(text)) => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn select_label_stays_unmarked_and_unrelated_controls_generate_no_text() {
    let out = render(
        "<select id='s'><option selected>Alpha</option></select>\
         <input id='n' type='number' value='42'>\
         <input id='c' type='checkbox' value='yes'>\
         <input id='b' type='submit' value='Go'>\
         <input id='e' type='text'>",
    );
    assert_eq!(generated_text(&out, "select").as_deref(), Some("Alpha"));
    fn collect_replaced<'a>(node: &'a LayoutBox, found: &mut Vec<&'a LayoutBox>) {
        if matches!(node.box_type, BoxType::Replaced(_)) {
            found.push(node);
        }
        for child in &node.children {
            collect_replaced(child, found);
        }
    }
    let mut controls = Vec::new();
    collect_replaced(&out.layout, &mut controls);
    let inputs: Vec<_> = controls
        .iter()
        .filter(|box_| {
            matches!(box_.box_type, BoxType::Replaced(id)
                if out.document.node(id).as_element().is_some_and(|el| el.local_name == "input"))
        })
        .collect();
    assert_eq!(inputs.len(), 4, "four input boxes in the tree");
    for box_ in inputs {
        let BoxType::Replaced(id) = box_.box_type else {
            unreachable!()
        };
        let el = out.document.node(id).as_element().expect("input element");
        let type_ = el.get_attribute("type").unwrap_or("text");
        assert!(
            box_.children.is_empty(),
            "no generated text for type={type_}, got {:?}",
            box_.children
        );
    }
}
