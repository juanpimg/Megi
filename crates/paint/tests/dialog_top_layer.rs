//! Paint and hit-test ordering tests for top-layer `<dialog>` and `::backdrop`.

use megi_css::{Color, ComputedStyle, Display, Position};
use megi_core::{Document, Namespace};
use megi_layout::{BoxType, Dimensions, LayoutBox, Rect};
use megi_paint::{
    DisplayItem, HitKind, RenderConfig, build_display_list_and_hit_regions, hit_test,
    rasterize_display_list_with_font_database,
};

fn elem(doc: &mut Document, parent: megi_core::NodeId, tag: &str) -> megi_core::NodeId {
    let id = doc.create_element(Namespace::Html, tag, vec![]);
    doc.append_child(parent, id);
    id
}

fn solid_style(color: Color, pos: Position, z: Option<i32>) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.background_color = color;
    style.position = pos;
    style.z_index = z;
    style
}

fn make_box(id: megi_core::NodeId, style: ComputedStyle, rect: Rect) -> LayoutBox {
    let mut b = LayoutBox::new(BoxType::BlockNode(id), Some(style));
    b.dimensions = Dimensions::default();
    b.dimensions.content = rect;
    b.position = b.style.as_ref().unwrap().position;
    b.z_index = b.style.as_ref().unwrap().z_index.unwrap_or(0);
    b.establishes_stacking_context = b.z_index != 0 || b.position == Position::Fixed;
    b
}

#[test]
fn top_layer_dialog_and_backdrop_paint_above_high_z_index_and_block_hits() {
    let mut doc = Document::new();
    let root = doc.root();
    let html = elem(&mut doc, root, "html");
    let body = elem(&mut doc, html, "body");
    let button = elem(&mut doc, body, "button");
    let overlay = elem(&mut doc, body, "div");
    let wrapper = elem(&mut doc, body, "div");
    let dialog = elem(&mut doc, wrapper, "dialog");

    let viewport = Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut root_box = make_box(
        html,
        solid_style(Color::WHITE, Position::Static, None),
        viewport,
    );
    let mut body_box = make_box(
        body,
        solid_style(Color::TRANSPARENT, Position::Static, None),
        viewport,
    );
    let button_box = make_box(
        button,
        solid_style(Color::new(0, 0, 255, 255), Position::Static, None),
        Rect::new(5.0, 5.0, 20.0, 20.0),
    );
    let overlay_box = make_box(
        overlay,
        solid_style(Color::new(0, 255, 0, 255), Position::Absolute, Some(99_999)),
        Rect::new(25.0, 25.0, 50.0, 50.0),
    );
    let mut wrapper_box = make_box(
        wrapper,
        solid_style(Color::TRANSPARENT, Position::Relative, Some(1)),
        viewport,
    );
    let mut dialog_box = make_box(
        dialog,
        solid_style(Color::new(255, 0, 0, 255), Position::Fixed, None),
        Rect::new(30.0, 30.0, 40.0, 40.0),
    );
    dialog_box.is_top_layer = true;
    dialog_box.top_layer_order = 1;
    dialog_box.backdrop_color = Some(Color::new(0, 0, 0, 128));
    dialog_box.establishes_stacking_context = true;

    wrapper_box.children.push(dialog_box);
    body_box.children.push(button_box);
    body_box.children.push(overlay_box);
    body_box.children.push(wrapper_box);
    root_box.children.push(body_box);

    let (list, regions) = build_display_list_and_hit_regions(&root_box, viewport);
    let pixmap = rasterize_display_list_with_font_database(
        &list,
        100,
        100,
        RenderConfig::OPAQUE_WHITE,
        megi_paint::bundled_font_database(),
    );

    // Center pixel inside dialog is red (dialog paints over the z-index: 99999 green overlay).
    assert_eq!(pixmap.pixels[50 * 100 + 50], Color::new(255, 0, 0, 255));

    // Pixel outside dialog (at 10,10 over the blue button) is darkened by the black 50% backdrop.
    assert!(
        pixmap.pixels[10 * 100 + 10].b < 160,
        "backdrop must dim blue button"
    );

    // Backdrop intercepts hit testing over the covered button.
    let hit = hit_test(&regions, 10.0, 10.0).expect("hit");
    assert_eq!(hit, dialog);
    let top_region = regions
        .iter()
        .rev()
        .find(|r| r.node_id == Some(dialog))
        .expect("dialog backdrop region");
    assert_eq!(top_region.kind, HitKind::Background);
    let _ = list
        .items
        .iter()
        .filter(|i| matches!(i, DisplayItem::SolidColor { .. }))
        .count();
}
