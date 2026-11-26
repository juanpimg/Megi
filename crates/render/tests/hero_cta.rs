//! Regression: a border-box flex item must not overflow the viewport when its
//! specified `width: 100%` is resolved inside a flex container (rust-lang.org
//! hero at 1024px: the yellow "Get Started" CTA was clipped on the right).
//!
//! CSS UI 3 §4: a specified size is the border box under
//! `box-sizing: border-box`; flex geometry tracks content boxes, so padding and
//! border must be stripped from both the main-size and cross-size inputs.

use std::path::PathBuf;

use megi_core::NodeId;
use megi_layout::LayoutBox;
use megi_render::{RenderOptions, Viewport, render_url};

const VIEWPORT_WIDTH: u32 = 1024;

fn find_element(doc: &megi_core::Document, class: &str) -> NodeId {
    let body = doc.body().expect("body");
    doc.descendants(body)
        .find(|id| {
            doc.try_node(*id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| {
                    el.attributes.iter().any(|a| {
                        a.local_name == "class" && a.value.split_whitespace().any(|c| c == class)
                    })
                })
        })
        .expect("element with class")
}

fn find_box(root: &LayoutBox, id: NodeId) -> Option<&LayoutBox> {
    match &root.box_type {
        megi_layout::BoxType::BlockNode(node)
        | megi_layout::BoxType::InlineBlock(node)
        | megi_layout::BoxType::InlineNode(node)
            if *node == id =>
        {
            return Some(root);
        }
        _ => {}
    }
    root.children.iter().find_map(|c| find_box(c, id))
}

fn right_edge(root: &LayoutBox, id: NodeId) -> f32 {
    let b = find_box(root, id).expect("layout box");
    let d = b.dimensions;
    d.content.x + d.content.width + d.padding.right + d.border.right
}

fn render_hero() -> megi_render::RenderOutput {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/fixtures/render/hero_cta.html");
    let url = format!(
        "file://{}",
        fixture.display().to_string().replace(' ', "%20")
    );
    let options = RenderOptions {
        viewport: Viewport::new(VIEWPORT_WIDTH, 768).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&url, &options).expect("render hero fixture")
}

#[test]
fn hero_cta_stays_inside_the_viewport() {
    let output = render_hero();
    let limit = VIEWPORT_WIDTH as f32;

    for class in ["button-download", "download-link"] {
        let id = find_element(&output.document, class);
        let right = right_edge(&output.layout, id);
        assert!(
            right <= limit,
            ".{class} right edge {right:.1} exceeds viewport {limit:.1}"
        );
    }
}

#[test]
fn hero_header_flex_item_keeps_its_padding_inside_the_viewport() {
    let output = render_hero();
    let header = find_element(&output.document, "mw9-l");
    let right = right_edge(&output.layout, header);
    assert!(
        right <= VIEWPORT_WIDTH as f32,
        "header border box right edge {right:.1} exceeds viewport"
    );
}
