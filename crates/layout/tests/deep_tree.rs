//! Deep layout-tree robustness tests (audit finding F6).
//!
//! The compiler-generated recursive `Drop` used to overflow the stack while freeing a
//! layout tree with tens of thousands of nested boxes. `LayoutBox` now has an explicit
//! iterative `Drop`; these tests grow a long chain in a small thread stack and drop it.

use std::collections::HashMap;

use css::{ComputedStyle, Display, Length};
use dom::{Document, Namespace};
use layout::{BoxType, LayoutBox, Rect, layout_document};

/// Builds a linked chain of anonymous layout boxes (no DOM required).
fn anonymous_chain(depth: usize) -> LayoutBox {
    let mut root = LayoutBox::new(BoxType::AnonymousBlock, None);
    for _ in 0..depth {
        let mut parent = LayoutBox::new(BoxType::AnonymousBlock, None);
        parent.children.push(root);
        root = parent;
    }
    root
}

#[test]
fn test_deep_layout_box_chain_drop_is_iterative() {
    // 50_000 nested boxes in a 256 KiB stack: a recursive drop would abort here.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let chain = anonymous_chain(50_000);
            drop(chain);
        })
        .expect("spawn deep-drop thread")
        .join()
        .expect("dropping a 50k-deep layout chain must not overflow the stack");
}

#[test]
fn test_deep_layout_box_chain_drop_with_partial_children() {
    // Same depth, but every level also carries a sibling leaf: the iterative drop must
    // walk breadth as well as depth without recursion.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut root = LayoutBox::new(BoxType::AnonymousBlock, None);
            for _ in 0..20_000 {
                let leaf = LayoutBox::new(BoxType::TextNode("x".to_string()), None);
                let mut parent = LayoutBox::new(BoxType::AnonymousBlock, None);
                parent.children.push(root);
                parent.children.push(leaf);
                root = parent;
            }
            drop(root);
        })
        .expect("spawn deep-drop thread")
        .join()
        .expect("dropping a 20k-deep chain with siblings must not overflow");
}

#[test]
fn test_deep_dom_layout_does_not_overflow_default_sized_stack() {
    // Tree construction, BFC layout and metadata finalization are still recursive
    // (an iterative layout pass is future work), so keep the depth far above real
    // pages but within a debug-build stack; the iterative `Drop` is covered above with
    // 20k/50k-deep chains.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut doc = Document::new();
            let html = doc.create_element(Namespace::Html, "html", vec![]);
            doc.append_child(doc.root(), html);
            let mut styles = HashMap::new();
            let mut html_style = ComputedStyle::initial();
            html_style.display = Display::Block;
            styles.insert(html, html_style);
            let mut parent = html;
            for _ in 0..200 {
                let div = doc.create_element(Namespace::Html, "div", vec![]);
                doc.append_child(parent, div);
                let mut style = ComputedStyle::initial();
                style.display = Display::Block;
                style.width = Length::Auto;
                styles.insert(div, style);
                parent = div;
            }
            let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 800.0, 600.0));
            assert!(tree.is_some(), "deep document must lay out");
            drop(tree);
        })
        .expect("spawn deep-layout thread")
        .join()
        .expect("deep document layout must not overflow the stack");
}
