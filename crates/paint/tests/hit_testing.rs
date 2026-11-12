//! Hit-testing tests (Phase 7 WP-7.5).
//!
//! The painter can optionally emit a parallel list of [`paint::HitRegion`]s
//! (painting order, already clipped) used by the shell for clicks/hover.
//! These tests pin: last-painted-wins ordering, clip awareness, node identity
//! for backgrounds/borders/text/images/markers/decorations, `<a href>` lookup
//! through ancestor chains, and full determinism.

use std::sync::Arc;

use css::{BorderStyle, Color, ComputedStyle, Display, TextDecoration};
use dom::{Document, Namespace};
use image::RgbaImage;
use layout::{BoxType, Dimensions, EdgeSizes, LayoutBox, Rect};
use paint::{
    HitKind, HitLink, ImageStore, build_display_list_and_hit_regions,
    build_display_list_with_hit_regions, find_link_at, hit_test, hit_test_all,
};

const VIEWPORT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 320.0,
    height: 200.0,
};

fn node_ids(count: usize) -> (Document, Vec<dom::NodeId>) {
    let mut document = Document::new();
    let html = document.create_element(Namespace::Html, "html", vec![]);
    document.append_child(document.root(), html);
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let id = document.create_element(Namespace::Html, "div", vec![]);
        document.append_child(html, id);
        ids.push(id);
    }
    (document, ids)
}

fn block_style() -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style
}

fn box_with_rect(id: dom::NodeId, style: ComputedStyle, rect: Rect) -> LayoutBox {
    let mut layout_box = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    layout_box.dimensions = Dimensions {
        content: rect,
        padding: EdgeSizes::ZERO,
        border: EdgeSizes::ZERO,
        margin: EdgeSizes::ZERO,
    };
    layout_box
}

fn transparent_root(id: dom::NodeId) -> LayoutBox {
    let mut root = LayoutBox::new(BoxType::BlockNode(id), Some(block_style()));
    root.dimensions.content = VIEWPORT;
    root
}

/// Renders an HTML document to (display list, hit regions) with the bundled
/// deterministic fonts, mirroring the real render pipeline's layout call.
fn html_regions(source: &str) -> (Document, paint::DisplayList, Vec<paint::HitRegion>) {
    let document = html::parse_document(source, false);
    let styles = paint::resolve_embedded_styles(&document);
    let measurer = layout::FontTextMeasurer::bundled().expect("bundled fonts");
    let layout_root =
        layout::layout_document_with_measurer(&document, &styles, VIEWPORT, &measurer)
            .expect("layout root");
    let (display_list, regions) = build_display_list_and_hit_regions(&layout_root, VIEWPORT);
    (document, display_list, regions)
}

#[test]
fn test_overlapping_regions_last_painted_wins() {
    let (_document, ids) = node_ids(3);
    let mut root = transparent_root(ids[0]);
    root.children.push(box_with_rect(
        ids[1],
        block_style(),
        Rect::new(0.0, 0.0, 100.0, 100.0),
    ));
    // Painted later (last child) and overlapping: wins inside the overlap.
    root.children.push(box_with_rect(
        ids[2],
        block_style(),
        Rect::new(50.0, 50.0, 100.0, 100.0),
    ));

    let (_list, regions) = build_display_list_and_hit_regions(&root, VIEWPORT);
    assert_eq!(hit_test(&regions, 70.0, 70.0), Some(ids[2]));
    assert_eq!(hit_test(&regions, 20.0, 20.0), Some(ids[1]));
    assert_eq!(hit_test(&regions, 400.0, 300.0), None);
}

#[test]
fn test_hit_test_all_returns_topmost_first() {
    let (_document, ids) = node_ids(3);
    let mut root = transparent_root(ids[0]);
    root.children.push(box_with_rect(
        ids[1],
        block_style(),
        Rect::new(0.0, 0.0, 100.0, 100.0),
    ));
    root.children.push(box_with_rect(
        ids[2],
        block_style(),
        Rect::new(10.0, 10.0, 50.0, 50.0),
    ));
    let (_list, regions) = build_display_list_and_hit_regions(&root, VIEWPORT);
    let hits = hit_test_all(&regions, 20.0, 20.0);
    let node_order: Vec<Option<dom::NodeId>> = hits.iter().map(|hit| hit.node_id).collect();
    assert_eq!(node_order.first(), Some(&Some(ids[2])), "topmost first");
    assert!(node_order.contains(&Some(ids[1])));
}

#[test]
fn test_clip_excludes_out_of_clip_regions() {
    let (_document, ids) = node_ids(3);
    let mut root = transparent_root(ids[0]);
    let mut clipper = box_with_rect(ids[1], block_style(), Rect::new(0.0, 0.0, 20.0, 20.0));
    clipper.clip = Some(Rect::new(0.0, 0.0, 20.0, 20.0));
    let mut inner = box_with_rect(ids[2], block_style(), Rect::new(0.0, 0.0, 40.0, 20.0));
    inner.clip = Some(Rect::new(0.0, 0.0, 20.0, 20.0));
    clipper.children.push(inner);
    root.children.push(clipper);

    let (_list, regions) = build_display_list_and_hit_regions(&root, VIEWPORT);
    assert_eq!(hit_test(&regions, 10.0, 10.0), Some(ids[2]), "inside clip");
    assert!(
        regions
            .iter()
            .filter(|region| region.node_id == Some(ids[2]))
            .all(|region| region.rect.x + region.rect.width <= 20.0 + 0.01),
        "clipped regions must be intersected with the clip at emission time"
    );
    assert_ne!(
        hit_test(&regions, 30.0, 10.0),
        Some(ids[2]),
        "pixels outside the clip must not report the clipped child"
    );
}

#[test]
fn test_text_and_decoration_regions_carry_owner_node() {
    let (_document, ids) = node_ids(2);
    let mut root = transparent_root(ids[0]);
    let mut text_style = block_style();
    text_style.display = Display::Inline;
    text_style.text_decoration = TextDecoration::UNDERLINE;
    let mut text = LayoutBox::new(BoxType::TextNode("hello".to_string()), Some(text_style));
    text.dimensions.content = Rect::new(10.0, 10.0, 40.0, 16.0);
    text.baseline = 12.0;
    let mut parent = box_with_rect(ids[1], block_style(), Rect::new(0.0, 0.0, 100.0, 50.0));
    parent.children.push(text);
    root.children.push(parent);

    let (_list, regions) = build_display_list_and_hit_regions(&root, VIEWPORT);
    let text_region = regions
        .iter()
        .find(|region| region.kind == HitKind::Text)
        .expect("text region");
    assert_eq!(
        text_region.node_id,
        Some(ids[1]),
        "text inherits owner element"
    );
    assert!(
        regions
            .iter()
            .any(|region| region.kind == HitKind::Decoration),
        "underline must produce a decoration region"
    );
    assert_eq!(hit_test(&regions, 20.0, 12.0), Some(ids[1]));
}

#[test]
fn test_image_and_border_regions_have_node_ids() {
    let (_document, ids) = node_ids(2);
    let mut root = transparent_root(ids[0]);
    let mut box_style = block_style();
    box_style.border_width = css::RectEdges::all(2.0);
    box_style.border_color = css::RectEdges::all(Color::RED);
    box_style.border_style = css::RectEdges::all(BorderStyle::Solid);
    let mut container = box_with_rect(ids[1], box_style, Rect::new(0.0, 0.0, 50.0, 50.0));
    container.dimensions.border = EdgeSizes::all(2.0);
    root.children.push(container);

    // Replaced image box with a 1x1 red bitmap.
    let image_node = ids[1];
    let mut image_box = LayoutBox::new(BoxType::Replaced(image_node), Some(block_style()));
    image_box.dimensions.content = Rect::new(60.0, 0.0, 20.0, 20.0);
    root.children.push(image_box);
    let mut store = ImageStore::new();
    let image = RgbaImage::new(1, 1, vec![255, 0, 0, 255]).expect("RGBA");
    store.insert(image_node, Arc::new(image));

    let (_list, regions) = build_display_list_with_hit_regions(
        &root,
        VIEWPORT,
        &store,
        &paint::BackgroundImages::new(),
    );
    assert!(
        regions
            .iter()
            .any(|region| region.kind == HitKind::Border && region.node_id == Some(ids[1]))
    );
    let image_region = regions
        .iter()
        .find(|region| region.kind == HitKind::Image)
        .expect("image region");
    assert_eq!(image_region.node_id, Some(image_node));
    assert_eq!(hit_test(&regions, 70.0, 10.0), Some(image_node));
}

#[test]
fn test_find_link_at_span_inside_anchor() {
    let (document, display_list, regions) = html_regions(
        "<!doctype html><html><body style=\"margin:0\">\
         <p style=\"margin:0\"><a href=\"/docs/page?q=1\"><span>click me</span> tail</a></p>\
         </body></html>",
    );
    assert!(!display_list.is_empty());
    let anchor_id = {
        let mut stack = vec![document.root()];
        let mut found = None;
        while let Some(node) = stack.pop() {
            if let Some(element) = document.node(node).as_element()
                && element.local_name == "a"
            {
                found = Some(node);
                break;
            }
            let children: Vec<dom::NodeId> = document.children(node).collect();
            for child in children {
                stack.push(child);
            }
        }
        found.expect("anchor element")
    };

    let text_region = regions
        .iter()
        .find(|region| region.kind == HitKind::Text && region.node_id.is_some())
        .expect("text region");
    let x = text_region.rect.x + text_region.rect.width / 2.0;
    let y = text_region.rect.y + text_region.rect.height / 2.0;
    let link: HitLink = find_link_at(&document, &regions, x, y).expect("link under the text");
    assert_eq!(link.node_id, anchor_id);
    assert_eq!(link.href, "/docs/page?q=1", "raw unresolved href");

    // Far outside the paragraph: nothing.
    assert_eq!(find_link_at(&document, &regions, 300.0, 190.0), None);
}

#[test]
fn test_find_link_at_requires_href() {
    let (document, _list, regions) = html_regions(
        "<!doctype html><html><body style=\"margin:0\">\
         <p style=\"margin:0\"><a name=\"x\">no href</a></p></body></html>",
    );
    let text_region = regions
        .iter()
        .find(|region| region.kind == HitKind::Text && region.node_id.is_some())
        .expect("text region");
    assert_eq!(
        find_link_at(
            &document,
            &regions,
            text_region.rect.x + 1.0,
            text_region.rect.y + 1.0
        ),
        None,
        "an <a> without href is not a link"
    );
}

#[test]
fn test_hit_regions_are_deterministic_and_do_not_change_display_list() {
    let (_document, ids) = node_ids(2);
    let mut root = transparent_root(ids[0]);
    root.children.push(box_with_rect(
        ids[1],
        block_style(),
        Rect::new(5.5, 2.25, 31.0, 17.5),
    ));

    let (list_with, first) = build_display_list_and_hit_regions(&root, VIEWPORT);
    let (list_without, second) = build_display_list_and_hit_regions(&root, VIEWPORT);
    assert_eq!(list_with.items, list_without.items, "regions are additive");
    assert_eq!(first, second, "regions must be deterministic");

    // Plain entry point stays available and identical in item count.
    let plain = paint::build_display_list(&root, VIEWPORT);
    assert_eq!(plain.items, list_with.items);
}

#[test]
fn test_rounded_overflow_clip_clips_child_hit_regions() {
    let (_document, ids) = node_ids(3);
    let mut root = transparent_root(ids[0]);

    let mut parent_style = block_style();
    parent_style.overflow = css::Overflow::Hidden;
    for corner in [
        &mut parent_style.border_radius.top_left,
        &mut parent_style.border_radius.top_right,
        &mut parent_style.border_radius.bottom_right,
        &mut parent_style.border_radius.bottom_left,
    ] {
        corner.x = css::Length::Px(8.0);
        corner.y = css::Length::Px(8.0);
    }
    let mut parent = box_with_rect(ids[1], parent_style, Rect::new(0.0, 0.0, 50.0, 50.0));

    let mut child_style = block_style();
    child_style.background_color = Color::RED;
    let child = box_with_rect(ids[2], child_style, Rect::new(0.0, 0.0, 120.0, 40.0));
    parent.children.push(child);
    root.children.push(parent);

    let (_list, regions) = build_display_list_and_hit_regions(&root, VIEWPORT);
    assert_eq!(hit_test(&regions, 25.0, 20.0), Some(ids[2]));
    assert_ne!(
        hit_test(&regions, 90.0, 20.0),
        Some(ids[2]),
        "child extending outside rounded overflow:hidden clip must not hit"
    );
}

#[test]
fn test_zero_alpha_text_does_not_record_hit_region() {
    let (_document, _list, regions) = html_regions(
        "<!doctype html><html><body style=\"margin:0\">\
         <div id=\"under\" style=\"width:100px;height:40px;background:red\">\
           <span id=\"zero-txt\" style=\"color:rgba(0,0,0,0)\">hidden</span>\
         </div>\
         </body></html>",
    );
    assert!(
        !regions.iter().any(|r| r.kind == HitKind::Text),
        "fully transparent text must not record HitKind::Text"
    );
}
