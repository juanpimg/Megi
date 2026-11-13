#![no_main]
//! Fuzz target: DOM mutation APIs (append/insert/clone/normalize/remove/compare).
//!
//! Drives Document mutators with attacker-influenced op streams over a bounded
//! arena of nodes to surface stale-NodeId, half-mutation and order bugs.

use dom::{Document, Namespace, NodeId};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut doc = Document::new();
    let root = doc.root();
    // Bounded arena: create up to 64 elements/text nodes.
    let mut nodes: Vec<NodeId> = vec![root];
    let mut i = 0;
    while i + 1 < data.len() && nodes.len() < 64 {
        let op = data[i];
        i += 1;
        match op % 8 {
            0 => {
                // create element
                let ns = if data[i] & 1 == 0 { Namespace::Html } else { Namespace::Svg };
                i += 1;
                let el = doc.create_element(ns, "div", Vec::new());
                nodes.push(el);
            }
            1 => {
                // create text node
                let text = String::from_utf8_lossy(&data[i..i + 1.min(data.len() - i)]);
                let tn = doc.create_text_node(&text);
                nodes.push(tn);
                i += 1;
            }
            2 => {
                // append_child
                if nodes.len() >= 2 {
                    let p = nodes[data[i] as usize % nodes.len()];
                    let c = nodes[data[(i + 1) % data.len()] as usize % nodes.len()];
                    i = i.wrapping_add(2);
                    let _ = doc.append_child(p, c);
                }
            }
            3 => {
                // insert_before
                if nodes.len() >= 3 {
                    let r = nodes[data[i] as usize % nodes.len()];
                    let n = nodes[data[(i + 1) % data.len()] as usize % nodes.len()];
                    i = i.wrapping_add(2);
                    let _ = doc.insert_before(r, n);
                }
            }
            4 => {
                // clone_node (shallow + deep)
                if !nodes.is_empty() {
                    let n = nodes[data[i] as usize % nodes.len()];
                    let deep = data[i] & 1 != 0;
                    i += 1;
                    let clone = doc.clone_node(n, deep);
                    if nodes.len() < 64 {
                        nodes.push(clone);
                    }
                }
            }
            5 => {
                // remove_subtree
                if nodes.len() > 1 {
                    let n = nodes[data[i] as usize % (nodes.len() - 1) + 1];
                    i += 1;
                    let _ = doc.remove_subtree(n);
                }
            }
            6 => {
                // normalize
                if !nodes.is_empty() {
                    let n = nodes[data[i] as usize % nodes.len()];
                    i += 1;
                    let _ = doc.normalize(n);
                }
            }
            7 => {
                // compare_document_position
                if nodes.len() >= 2 {
                    let a = nodes[data[i] as usize % nodes.len()];
                    let b = nodes[data[(i + 1) % data.len()] as usize % nodes.len()];
                    i = i.wrapping_add(2);
                    let _ = doc.compare_document_position(a, b);
                }
            }
            _ => {}
        }
    }
    // Validate invariants: no panic on traversal.
    let _ = doc.children(root);
});
