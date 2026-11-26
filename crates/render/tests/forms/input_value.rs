//! Visual regression coverage for native `<input>` text painting.
//!
//! WHATWG HTML §4.10.5 (text/search state: the `value` and `placeholder` IDL
//! attributes control the control's rendered text) / CSS UI §3 (UA form-control
//! appearance, padding/border and text overflow inside the content box).
//!
//! The fixture mirrors the control text into a marked generated text child,
//! which is the same layout→paint channel `<select>` already uses for its
//! selected-option label. Paint must pick the right text, inherit typography,
//! clip to the content box and leave `<select>` untouched.

use megi_core::Rect;
use megi_display::DisplayItem;
use megi_layout::{BoxType, LayoutBox};
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

const FIXTURE: &str = include_str!("fixtures/input_value_paint.html");
const GOLDEN: &[u8] = include_bytes!("golden/input_value_paint.png");

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

fn render_fixture() -> RenderOutput {
    render_url(
        &html_data_url(FIXTURE),
        &RenderOptions {
            viewport: Viewport::new(640, 220).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render fixture")
}

fn node_id_for_id(out: &RenderOutput, wanted: &str) -> megi_core::NodeId {
    out.document
        .descendants(out.document.root())
        .find(|id| {
            out.document
                .node(*id)
                .as_element()
                .and_then(|el| el.get_attribute("id"))
                .is_some_and(|id| id == wanted)
        })
        .unwrap_or_else(|| panic!("no element with id {wanted}"))
}

fn replaced_box(node: &LayoutBox, id: megi_core::NodeId) -> Option<&LayoutBox> {
    if matches!(node.box_type, BoxType::Replaced(box_id) if box_id == id) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| replaced_box(child, id))
}

fn control_box(out: &RenderOutput, id: &str) -> LayoutBox {
    replaced_box(&out.layout, node_id_for_id(out, id))
        .unwrap_or_else(|| panic!("no replaced box for #{id}"))
        .clone()
}

fn text_item<'a>(out: &'a RenderOutput, wanted: &str) -> Option<&'a DisplayItem> {
    out.display_list
        .items
        .iter()
        .find(|item| matches!(item, DisplayItem::Text { text, .. } if text == wanted))
}

fn dark_pixels(out: &RenderOutput, rect: Rect) -> usize {
    let x0 = rect.x.max(0.0) as u32;
    let y0 = rect.y.max(0.0) as u32;
    let x1 = (rect.x + rect.width).min(out.pixmap.width as f32) as u32;
    let y1 = (rect.y + rect.height).min(out.pixmap.height as f32) as u32;
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let color = out.pixmap.pixels[y as usize * out.pixmap.width as usize + x as usize];
            if color.a > 0 && color.r < 160 && color.g < 160 && color.b < 160 {
                count += 1;
            }
        }
    }
    count
}

/// HTML §4.10.5: the value is what the user sees in a text control.
#[test]
fn input_value_reaches_display_list_and_rasterizes_text() {
    let out = render_fixture();
    let control = control_box(&out, "filled");
    let item = text_item(&out, "Megi value")
        .unwrap_or_else(|| panic!("input value text missing; got {:?}", out.display_list.items));
    let DisplayItem::Text { rect, color, .. } = item else {
        unreachable!()
    };
    assert!(
        rect == &control.dimensions.content,
        "text must fill the content box, got {rect:?} vs {:?}",
        control.dimensions.content
    );
    assert!(
        color.r < 64 && color.g < 64 && color.b < 64,
        "value must inherit the author color"
    );
    assert!(
        dark_pixels(&out, control.dimensions.content) >= 40,
        "value glyphs must appear as dark pixels inside the content box"
    );
}

/// HTML §4.10.5: placeholder is shown only while the value is empty.
#[test]
fn input_placeholder_is_used_when_value_is_empty() {
    let out = render_fixture();
    let item = text_item(&out, "Search Megi")
        .unwrap_or_else(|| panic!("placeholder text missing; got {:?}", out.display_list.items));
    let DisplayItem::Text { color, .. } = item else {
        unreachable!()
    };
    assert!(
        color.r >= 96 && color.r <= 160 && color.r == color.g && color.g == color.b,
        "placeholder must use the UA grey, got {color:?}"
    );
    assert!(text_item(&out, "Megi value").is_some());
}

/// CSS UI / HTML §4.10.5: long control text is clipped to the content box.
#[test]
fn input_text_is_clipped_inside_content_box() {
    let out = render_fixture();
    let control = control_box(&out, "long");
    let content = control.dimensions.content;
    let padding_right = control.dimensions.padding_box();
    let gutter = Rect::new(
        content.x + content.width,
        content.y,
        (padding_right.x + padding_right.width - content.x - content.width).max(0.0),
        content.height,
    );
    assert!(
        gutter.width > 0.0,
        "fixture must leave a right padding gutter"
    );
    assert_eq!(
        dark_pixels(&out, gutter),
        0,
        "no glyph may spill out of the content box into padding"
    );
    let DisplayItem::Text { rect, .. } = text_item(&out, &"W".repeat(50)).expect("long value")
    else {
        unreachable!()
    };
    assert_eq!(rect, &content);
    assert!(
        out.display_list.items.iter().any(|item| matches!(
            item,
            DisplayItem::PushClip { rect } if *rect == content
        )),
        "input text must be pushed through a content-box clip"
    );
}

/// The native select path (label + arrow, no self-clip) must stay unchanged.
#[test]
fn select_path_is_not_mistaken_for_input_text() {
    let out = render(
        "<!doctype html><body style='margin:0'>\
         <select><option selected>Alpha</option></select></body>",
    );
    assert!(
        text_item(&out, "Alpha").is_some(),
        "select label must still paint"
    );
    assert!(
        !out.display_list
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::PushClip { .. })),
        "select must not gain a content-box clip"
    );
    let arrows = out
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayItem::SolidColor { color, .. } if color.r == 51 && color.g == 51 && color.b == 51))
        .count();
    assert!(arrows >= 5, "select arrow scanlines must survive");
}

fn render(html: &str) -> RenderOutput {
    render_url(
        &html_data_url(html),
        &RenderOptions {
            viewport: Viewport::new(800, 200).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

#[test]
fn fixture_png_is_byte_identical_to_golden() {
    let out = render_fixture();
    assert_eq!(
        out.png.as_slice(),
        GOLDEN,
        "fixture PNG drifted from golden"
    );
}
