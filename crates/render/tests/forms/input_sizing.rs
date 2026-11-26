//! Regression: form controls sized by HTML attributes (`size`) or CSS
//! (`width`/`height`) must keep that geometry instead of collapsing to the UA
//! `width:10em` / `max-width:100%` default (#2247 — google.com's search field
//! uses `size=57` and appeared as a ~15px grey bar).
//!
//! WHATWG HTML §4.10.5.1.2 (`size` attribute sizes a text control in average
//! character advances) / CSS UI 3 §4 (`box-sizing: border-box` on controls).

use megi_core::Rect;
use megi_layout::{BoxType, LayoutBox};
use megi_render::{RenderOptions, Viewport, render_url};

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

fn render_with_viewport(html: &str, width: u32, height: u32) -> megi_render::RenderOutput {
    render_url(
        &html_data_url(html),
        &RenderOptions {
            viewport: Viewport::new(width, height).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

fn render(html: &str) -> megi_render::RenderOutput {
    render_with_viewport(html, 800, 200)
}

fn replaced_box(box_node: &LayoutBox) -> Option<&LayoutBox> {
    if matches!(box_node.box_type, BoxType::Replaced(_)) {
        return Some(box_node);
    }
    box_node.children.iter().find_map(replaced_box)
}

fn control_rect(html: &str) -> Rect {
    let out = render(html);
    replaced_box(&out.layout)
        .expect("form control replaced box")
        .dimensions
        .border_box()
}

fn contains_label(box_node: &LayoutBox, label: &str) -> bool {
    box_node.children.iter().any(|child| {
        matches!(&child.box_type, BoxType::TextNode(text) if text == label)
            || contains_label(child, label)
    })
}

fn inline_block_with_label<'a>(box_node: &'a LayoutBox, label: &str) -> Option<&'a LayoutBox> {
    if matches!(box_node.box_type, BoxType::InlineBlock(_)) && contains_label(box_node, label) {
        return Some(box_node);
    }
    box_node
        .children
        .iter()
        .find_map(|child| inline_block_with_label(child, label))
}

/// HTML §4.10.5.1.2: without an author `width`, a text field's width comes
/// from `size` (~8px per average character), not the UA `width:10em` cap.
#[test]
fn input_size_attribute_drives_width_without_author_css() {
    let rect = control_rect(
        "<!doctype html><body style='margin:0'>\
         <input type='search' size='57'></body>",
    );
    assert!(
        rect.width > 300.0,
        "input size=57 width {} collapsed to the UA 10em (~160px); \
         HTML §4.10.5.1.2 sizes it in average character advances",
        rect.width
    );
}

/// A larger `size` must produce a proportionally wider control than a small one.
#[test]
fn input_width_scales_with_size_attribute() {
    let small = control_rect(
        "<!doctype html><body style='margin:0'>\
         <input type='text' size='10'></body>",
    );
    let large = control_rect(
        "<!doctype html><body style='margin:0'>\
         <input type='text' size='57'></body>",
    );
    assert!(
        large.width > small.width + 200.0,
        "size=57 ({}) must be much wider than size=10 ({})",
        large.width,
        small.width
    );
}

/// CSS UI 3 §4: an author `width` is the border box under the UA
/// `box-sizing:border-box` and must survive alongside a `size` attribute.
#[test]
fn input_author_width_wins_over_size_attribute() {
    let rect = control_rect(
        "<!doctype html><body style='margin:0'>\
         <input type='text' size='57' style='width:496px;height:30px'></body>",
    );
    assert!(
        (rect.width - 496.0).abs() < 1.0,
        "author width:496px must win over size=57, got {}",
        rect.width
    );
}

/// An authored height must be kept: a one-line control must not collapse to
/// the UA `1.6em` default when the page asks for more room.
#[test]
fn input_author_height_is_kept() {
    let rect = control_rect(
        "<!doctype html><body style='margin:0'>\
         <input type='search' size='20' style='height:40px'></body>",
    );
    assert!(
        (rect.height - 40.0).abs() < 1.0,
        "author height:40px must be kept, got {}",
        rect.height
    );
}

/// The UA `max-width:100%` resolves against the narrow inline formatting
/// context of an atomic inline (CSS 2.2 §10.3.2). A `size`-driven control must
/// not collapse to the width of a tiny wrapper: that is the reported ~15px bar.
#[test]
fn input_size_attribute_survives_narrow_inline_context() {
    let rect = control_rect(
        "<!doctype html><body style='margin:0'>\
         <div style='width:15px'><input type='text' size='57'></div></body>",
    );
    assert!(
        rect.width > 100.0,
        "size=57 collapsed to {} inside a narrow IFC (UA max-width:100% still applied)",
        rect.width
    );
}

/// CSS UI 3 §4: an inline-block button keeps an authored border box and its
/// label is a visible text child (not a clipped or replaced grey box).
#[test]
fn button_author_size_is_kept_and_label_is_present() {
    let out = render(
        "<!doctype html><body style='margin:0'>\
         <button style='width:200px;height:40px'>Search</button></body>",
    );
    let button =
        inline_block_with_label(&out.layout, "Search").expect("button box with a label child");
    let rect = button.dimensions.border_box();
    assert!(
        (rect.width - 200.0).abs() < 1.0 && (rect.height - 40.0).abs() < 1.0,
        "button border box {rect:?} must be 200x40"
    );
}

/// The same authored geometry applies to `<select>` (an atomic inline) while
/// the disclosure arrow stays inside its border box.
#[test]
fn select_author_size_is_kept() {
    let rect = control_rect(
        "<!doctype html><body style='margin:0'>\
         <select style='width:200px;height:40px'><option>short</option></select></body>",
    );
    assert!(
        (rect.width - 200.0).abs() < 1.0 && (rect.height - 40.0).abs() < 1.0,
        "select border box {rect:?} must be 200x40"
    );
}

/// Goldens: the local fixture exercises all three sizing paths at once.
/// Before the fix: md5 43dcaab4302036b717a3c9dd840e05ac (all size-driven
/// fields stuck at ~160px). After: md5 ae3af433ef4781da045fb86a1ed8f128.
#[test]
fn fixture_png_is_byte_identical_to_golden() {
    let out = render_with_viewport(include_str!("fixtures/input_sizing.html"), 800, 260);
    let golden = include_bytes!("golden/input_sizing.png");
    assert_eq!(
        out.png.len(),
        golden.len(),
        "fixture PNG length changed; regenerate golden/input_sizing.png"
    );
    assert!(
        out.png == golden,
        "fixture PNG bytes changed; check the form-control geometry"
    );
}
