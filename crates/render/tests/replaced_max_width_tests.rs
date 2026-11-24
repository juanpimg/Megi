//! #2247: the UA `max-width:100%` cap on form controls must not clip an
//! atomic inline with an author `width`, while an author `max-width:100%`
//! keeps capping. These tests drive the real HTML -> cascade -> layout path.

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

fn render(html: &str) -> megi_render::RenderOutput {
    render_url(
        &html_data_url(html),
        &RenderOptions {
            viewport: Viewport::new(800, 200).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render")
}

/// Border-box width of the single replaced box (the `<input>`) in the laid-out
/// tree. The UA stylesheet sets `box-sizing:border-box` on form controls, so
/// the authored `width:496px` is the border box, not the content box.
fn replaced_border_width(box_node: &LayoutBox) -> Option<f32> {
    if matches!(box_node.box_type, BoxType::Replaced(_)) {
        return Some(box_node.dimensions.border_box().width);
    }
    box_node.children.iter().find_map(replaced_border_width)
}

fn input_width(html: &str) -> f32 {
    let out = render(html);
    replaced_border_width(&out.layout).expect("input replaced box")
}

/// WHATWG HTML §15.3.2 defines the UA stylesheet; the engine's UA rule
/// `input, textarea, select, button { max-width: 100% }` is meant to keep a
/// *block-level* form control inside its containing block. For an atomic
/// inline, the percentage resolves against the narrow inline formatting
/// context (CSS 2.2 §10.3.2), so it must not override the author's explicit
/// width (#2247).
#[test]
fn ua_max_width_cap_does_not_clip_atomic_inline_author_width() {
    let width = input_width(
        "<!doctype html><body style='margin:0'>\
         <div style='display:inline-block'>\
         <input style='width:496px;height:25px'>\
         </div></body>",
    );
    assert!(
        (width - 496.0).abs() < 1.0,
        "UA max-width:100% clipped the author width: got {width}, expected ~496"
    );
}

/// Guard: an *author* `max-width:100%` is not the UA cap and must keep
/// applying, even on the same atomic inline. Dropping every `100%` cap would
/// be an over-broad fix.
#[test]
fn author_max_width_100_percent_still_caps_atomic_inline() {
    let width = input_width(
        "<!doctype html><body style='margin:0'>\
         <div style='display:inline-block'>\
         <input style='width:496px;height:25px;max-width:100%'>\
         </div></body>",
    );
    assert!(
        width < 100.0,
        "author max-width:100% must still cap the input, got {width}"
    );
}
