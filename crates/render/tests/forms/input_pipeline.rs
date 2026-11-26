//! End-to-end native input text: plain HTML, no hand-written generated children.
//!
//! WHATWG HTML §4.10.5: text/search/email controls render their `value`, or
//! `placeholder` while empty; password controls mask the value with bullets.
//! CSS UI §3: the text lives inside the control's padding/border box.

use megi_core::Rect;
use megi_display::DisplayItem;
use megi_layout::BoxType;
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

const GOLDEN: &[u8] = include_bytes!("golden/input_pipeline.png");

const HTML: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
html, body { margin: 0; padding: 0; }
body { background: #fff; }
input {
  display: block; box-sizing: border-box;
  width: 240px; height: 34px; margin: 8px;
  padding: 4px 8px; border: 2px solid #666;
  background: #fff; color: #111;
  font: 16px/1.25 sans-serif;
}
</style></head><body>
<input id="value" type="text" value="Real pipeline">
<input id="placeholder" type="search" placeholder="Real placeholder">
<input id="email" type="email" value="user@example.com">
<input id="password" type="password" value="secret">
</body></html>"#;

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

fn render_page() -> RenderOutput {
    render_url(
        &html_data_url(HTML),
        &RenderOptions {
            viewport: Viewport::new(640, 260).expect("viewport"),
            ..RenderOptions::default()
        },
    )
    .expect("render plain HTML")
}

fn node_id(out: &RenderOutput, wanted: &str) -> megi_core::NodeId {
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

fn replaced_box(
    node: &megi_layout::LayoutBox,
    id: megi_core::NodeId,
) -> Option<&megi_layout::LayoutBox> {
    if matches!(node.box_type, BoxType::Replaced(box_id) if box_id == id) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| replaced_box(child, id))
}

fn control(out: &RenderOutput, id: &str) -> megi_layout::LayoutBox {
    replaced_box(&out.layout, node_id(out, id))
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
            let c = out.pixmap.pixels[y as usize * out.pixmap.width as usize + x as usize];
            if c.a > 0 && c.r < 160 && c.g < 160 && c.b < 160 {
                count += 1;
            }
        }
    }
    count
}

/// HTML §4.10.5: layout turns the content attribute into generated control
/// text; the paint pass receives it without any fixture-side markup.
#[test]
fn plain_input_value_is_generated_by_layout_and_painted() {
    let out = render_page();
    let box_ = control(&out, "value");
    let Some(BoxType::TextNode(text)) = box_.children.first().map(|c| &c.box_type) else {
        panic!("input has no generated text child: {:?}", box_.children)
    };
    assert_eq!(
        text, "\u{E000}Real pipeline",
        "layout carries a marked value run; paint strips the marker"
    );
    let item = text_item(&out, "Real pipeline").expect("value text in display list");
    let DisplayItem::Text { rect, .. } = item else {
        unreachable!()
    };
    assert_eq!(rect, &box_.dimensions.content);
    assert!(
        dark_pixels(&out, box_.dimensions.content) >= 40,
        "value glyphs must rasterize inside the content box"
    );
}

#[test]
fn plain_input_placeholder_is_generated_only_when_value_is_empty() {
    let out = render_page();
    let box_ = control(&out, "placeholder");
    let Some(BoxType::TextNode(text)) = box_.children.first().map(|c| &c.box_type) else {
        panic!("input has no generated text child: {:?}", box_.children)
    };
    assert_eq!(text, "\u{E001}Real placeholder");
    let DisplayItem::Text { color, .. } =
        text_item(&out, "Real placeholder").expect("placeholder text")
    else {
        unreachable!()
    };
    assert!(color.r == color.g && color.g == color.b && (96..=160).contains(&color.r));
}

#[test]
fn email_value_is_generated_like_text() {
    let out = render_page();
    assert!(text_item(&out, "user@example.com").is_some());
}

/// HTML §4.10.5 password state: the real value must never reach the display list.
#[test]
fn password_value_is_masked_with_bullets() {
    let out = render_page();
    let box_ = control(&out, "password");
    let Some(BoxType::TextNode(text)) = box_.children.first().map(|c| &c.box_type) else {
        panic!("password has no generated text child")
    };
    assert_eq!(text, "\u{E000}••••••");
    assert!(
        text_item(&out, "secret").is_none(),
        "raw password must not be painted"
    );
    assert!(text_item(&out, "••••••").is_some());
}

#[test]
fn pipeline_png_is_byte_identical_to_golden() {
    let out = render_page();
    assert_eq!(
        out.png.as_slice(),
        GOLDEN,
        "pipeline PNG drifted from golden"
    );
}
