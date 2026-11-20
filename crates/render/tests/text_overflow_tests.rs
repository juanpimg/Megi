//! #1929 end-to-end: `text-overflow: ellipsis` in the real render pipeline.
//!
//! The HTML from the issue is rendered through `render_url` (HTML → CSS → layout →
//! display list → PNG). The test checks both the painted text run and the encoded
//! PNG: the ellipsis glyph must be present and the whole run must fit the 60px box.

use megi_display::DisplayItem;
use megi_image::{PngLimits, RgbaImage, decode_png};
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

fn data_url(html: &str) -> String {
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~ ".contains(&byte) {
                if byte == b' ' {
                    "%20".to_string()
                } else {
                    (byte as char).to_string()
                }
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("data:text/html,{encoded}")
}

fn render(html: &str) -> RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(200, 40).expect("viewport"),
        scripting: megi_render::ScriptingMode::Disabled,
        ..RenderOptions::default()
    };
    render_url(&data_url(html), &options).expect("render")
}

fn issue_page() -> String {
    "<!DOCTYPE html><html><body><div style=\"width:60px;white-space:nowrap;\
     overflow:hidden;text-overflow:ellipsis\">abcdefghijklmnop</div></body></html>"
        .to_string()
}

/// Columns containing non-background ink in the given row band.
fn ink_columns(image: &RgbaImage, x0: usize, x1: usize, y0: usize, y1: usize) -> Vec<bool> {
    let mut columns = vec![false; x1 - x0];
    for y in y0..y1.min(image.height as usize) {
        for x in x0..x1.min(image.width as usize) {
            let index = (y * image.width as usize + x) * 4;
            let r = image.data[index];
            let g = image.data[index + 1];
            let b = image.data[index + 2];
            if r < 200 || g < 200 || b < 200 {
                columns[x - x0] = true;
            }
        }
    }
    columns
}

#[test]
fn ellipsis_run_is_painted_and_fits_the_clipped_box() {
    let output = render(&issue_page());
    let (text, rect) = output
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text { text, rect, .. } if text.contains('…') => {
                Some((text.clone(), *rect))
            }
            _ => None,
        })
        .expect("a text run ending in the ellipsis glyph must reach the display list");

    assert!(text.ends_with('…'), "unexpected ellipsized run: {text:?}");
    assert!(
        text.len() < "abcdefghijklmnop".len(),
        "run must be truncated"
    );
    // `body` has an 8px default margin; the box spans x=8..68.
    let box_left = 8.0_f32;
    let box_right = 68.0_f32;
    assert!(
        rect.x >= box_left - 0.01 && rect.x + rect.width <= box_right + 0.01,
        "painted run must fit inside the 60px box: {rect:?}"
    );

    let image = decode_png(&output.png, &PngLimits::default()).expect("decode pipeline PNG");
    let ink = ink_columns(&image, 8, 68, 0, image.height as usize);
    let last_ink = ink
        .iter()
        .rposition(|has_ink| *has_ink)
        .expect("visible text");
    assert!(last_ink + 8 < 68, "glyphs must fit before the clip edge");

    // Control page: the exact same ellipsized string in a wide, unclipped box.
    // Its ink pattern must match the clipped page pixel for pixel, proving the PNG
    // really contains the ellipsis glyph (not a half-cut 'f').
    let control_html =
        format!("<!DOCTYPE html><html><body><div style=\"width:200px\">{text}</div></body></html>");
    let control = render(&control_html);
    let control_image =
        decode_png(&control.png, &PngLimits::default()).expect("decode control PNG");
    assert_eq!(
        ink_columns(&image, 8, 68, 0, image.height as usize),
        ink_columns(&control_image, 8, 68, 0, control_image.height as usize),
        "clipped page must render the same glyphs as the ellipsized control"
    );
}

#[test]
fn ellipsis_is_not_shown_when_overflow_is_visible() {
    let html = "<!DOCTYPE html><html><body><div style=\"width:60px;white-space:nowrap;\
                overflow:visible;text-overflow:ellipsis\">abcdefghijklmnop</div></body></html>";
    let output = render(html);
    assert!(
        output.display_list.items.iter().all(|item| match item {
            DisplayItem::Text { text, .. } => !text.contains('…'),
            _ => true,
        }),
        "CSS UI 3 §4.1: text-overflow does not apply to overflow:visible"
    );
}
