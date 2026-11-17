#![no_main]
//! Direct fuzz target for `dom::Document` tree mutations (#834).

use dom::{Attribute, Document, Namespace};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);

    let mut nodes = vec![body];
    for chunk in data.chunks(4).take(128) {
        let op = chunk[0] % 5;
        let a = chunk.get(1).copied().unwrap_or(0) as usize;
        let b = chunk.get(2).copied().unwrap_or(0) as usize;
        match op {
            0 => {
                let tag = match b % 4 {
                    0 => "div",
                    1 => "span",
                    2 => "p",
                    _ => "a",
                };
                let el = doc.create_element(
                    Namespace::Html,
                    tag,
                    vec![Attribute::new("id", format!("n{a}"))],
                );
                let parent = nodes[a % nodes.len()];
                doc.append_child(parent, el);
                nodes.push(el);
            }
            1 => {
                let txt = doc.create_text_node("fuzz text");
                let parent = nodes[a % nodes.len()];
                doc.append_child(parent, txt);
            }
            2 if nodes.len() > 1 => {
                let idx = 1 + (a % (nodes.len() - 1));
                let target = nodes[idx];
                let new_parent = nodes[b % nodes.len()];
                if target != new_parent {
                    let _ = doc.append_child(new_parent, target);
                }
            }
            3 if nodes.len() > 1 => {
                let idx = 1 + (a % (nodes.len() - 1));
                let target = nodes[idx];
                if let Some(parent) = doc.node(target).parent {
                    let _ = doc.remove_child(parent, target);
                }
            }
            _ => {
                let target = nodes[a % nodes.len()];
                let _ = doc.children(target).count();
            }
        }
    }
    let _ = doc.children(doc.root()).count();
});
