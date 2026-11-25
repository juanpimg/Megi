//! P1 regression: `<select>` used to inherit the UA `width:8em; overflow:hidden`
//! rule and was painted as a generic replaced element (a grey box), with no
//! selected-option label and no arrow. These tests drive the real HTML →
//! cascade → layout → paint → display-list pipeline.
//!
//! WHATWG HTML §15.4 (UA stylesheet) / §13.2.6 (`select` element).

use megi_core::Rect;
use megi_display::DisplayItem;
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

fn replaced_box(box_node: &LayoutBox) -> Option<&LayoutBox> {
    if matches!(box_node.box_type, BoxType::Replaced(_)) {
        return Some(box_node);
    }
    box_node.children.iter().find_map(replaced_box)
}

fn select_border_box(html: &str) -> Rect {
    let out = render(html);
    replaced_box(&out.layout)
        .expect("select replaced box")
        .dimensions
        .border_box()
}

/// HTML §15.4: a select's width must follow its options, not a fixed `8em`.
#[test]
fn select_width_tracks_longest_option() {
    let rect = select_border_box(
        "<!doctype html><body style='margin:0'>\
         <select><option>A</option>\
         <option>This is a rather long option label</option></select></body>",
    );
    assert!(
        rect.width > 180.0,
        "select width {} must follow the longest option, not the UA 8em (~128px)",
        rect.width
    );
}

/// The UA `max-width:100%` cap still confines the intrinsic width to its container.
#[test]
fn select_width_is_capped_by_container() {
    let rect = select_border_box(
        "<!doctype html><body style='margin:0'>\
         <div style='width:120px'><select>\
         <option>This is a rather long option label</option>\
         </select></div></body>",
    );
    assert!(
        rect.width <= 121.0,
        "select width {} must not exceed its 120px container",
        rect.width
    );
}

/// The selected option's text is the control's only native content.
#[test]
fn select_paints_selected_option_text() {
    let out = render(
        "<!doctype html><body style='margin:0'>\
         <select><option>Alpha</option><option selected>Beta Gamma</option></select></body>",
    );
    assert!(
        out.display_list
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "Beta Gamma")),
        "selected option label must reach the display list; got {:?}",
        out.display_list.items
    );
}

/// The arrow is drawn analytically, inside the control, not as a grey image placeholder.
#[test]
fn select_draws_arrow_inside_control() {
    let html = "<!doctype html><body style='margin:0'>\
                <select><option>Short</option></select></body>";
    let out = render(html);
    let rect = replaced_box(&out.layout)
        .expect("select replaced box")
        .dimensions
        .border_box();
    let arrow_scanlines = out
        .display_list
        .items
        .iter()
        .filter(|item| {
            matches!(
                item,
                DisplayItem::SolidColor { rect: r, color }
                    if r.x >= rect.x + rect.width - 20.0
                        && r.x + r.width <= rect.x + rect.width
                        && r.y >= rect.y
                        && r.y + r.height <= rect.y + rect.height
                        && r.width > 0.0 && r.width <= 12.0
                        && r.height > 0.0 && r.height <= 2.0
                        && color.r < 128 && color.a > 0
            )
        })
        .count();
    assert!(
        arrow_scanlines >= 3,
        "expected >=3 dark arrow scanlines inside the select, got {arrow_scanlines}"
    );
}

/// `overflow:visible` on the control means no clip item around it, and the
/// neighbouring CTA keeps its own text and is never covered by the select.
#[test]
fn select_does_not_clip_neighbor_cta() {
    let out = render(
        "<!doctype html><body style='margin:0'><div style='white-space:nowrap'>\
         <select><option>A</option><option>This is a rather long option label</option></select>\
         <button type='button'>Get Started</button></div></body>",
    );
    let select = replaced_box(&out.layout).expect("select replaced box");
    let select_rect = select.dimensions.border_box();
    let select_content = select.dimensions.content;
    let select_padding = select.dimensions.padding_box();
    let cta_rect = out.display_list.items.iter().find_map(|item| match item {
        DisplayItem::Text { text, rect, .. } if text == "Get Started" => Some(*rect),
        _ => None,
    });
    let cta_rect = cta_rect.expect("neighbour CTA text");
    assert!(
        cta_rect.x >= select_rect.x + select_rect.width - 1.0,
        "CTA at {:?} overlaps select {:?}",
        cta_rect,
        select_rect
    );
    let self_clip = out.display_list.items.iter().any(|item| {
        matches!(
            item,
            DisplayItem::PushClip { rect }
                if *rect == select_rect
                    || *rect == select_content
                    || *rect == select_padding
        )
    });
    assert!(
        !self_clip,
        "select must use overflow:visible, not clip its own box ({select_rect:?} / {select_padding:?} / {select_content:?})"
    );
}
