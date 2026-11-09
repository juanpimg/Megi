//! # Megi 2D Painting & Display List Engine (`paint`)
//!
//! Generates decoupled CSS Display Lists according to CSS 2.1 Appendix E painting order
//! and performs pure safe Rust 2D rasterization into an RGBA pixel buffer ([`Pixmap`]).
//!
//! The crate also owns the Phase 4 **HTML → PNG** building blocks:
//!
//! - [`build_display_list`] turns a laid-out [`layout::LayoutBox`] tree into a
//!   flat display list, painting the canvas background following CSS 2.2 §14.2
//!   (root/`body` background propagation).
//! - [`rasterize_display_list`] executes the list into a [`Pixmap`] starting
//!   from an opaque white canvas (configurable through [`RenderConfig`]).
//! - [`Pixmap::to_png`] encodes the buffer as a valid PNG with a
//!   dependency-free, deterministic writer (see [`png`]).
//! - [`collect_style_text`] / [`resolve_embedded_styles`] extract and resolve
//!   the author CSS embedded in `<style>` elements.
//!
//! See `crates/paint/examples/html_to_png.rs` for the end-to-end driver.

pub mod display_list;
pub mod painter;
pub mod png;
pub mod rasterizer;
pub mod style;

pub use css::Color;
pub use display_list::{DisplayItem, DisplayList};
pub use painter::build_display_list;
pub use png::encode_rgba;
pub use rasterizer::{
    Pixmap, RenderConfig, rasterize_display_list, rasterize_display_list_with_config,
};
pub use style::{collect_style_text, resolve_embedded_styles};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use css::{ComputedStyle, Display, Length};
    use dom::{Document, Namespace};
    use layout::{Rect, layout_document};

    fn make_box_style(
        width: Length,
        height: Length,
        bg_color: Color,
        border_width: f32,
        border_color: Color,
    ) -> ComputedStyle {
        let mut s = ComputedStyle::initial();
        s.display = Display::Block;
        s.width = width;
        s.height = height;
        s.background_color = bg_color;
        s.border_width.top = border_width;
        s.border_width.right = border_width;
        s.border_width.bottom = border_width;
        s.border_width.left = border_width;
        s.border_color.top = border_color;
        s.border_color.right = border_color;
        s.border_color.bottom = border_color;
        s.border_color.left = border_color;
        s.border_style = css::RectEdges::all(css::BorderStyle::Solid);
        s
    }

    #[test]
    fn test_display_list_generation_from_layout_tree() {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        let span = doc.create_element(Namespace::Html, "span", vec![]);

        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, div);
        doc.append_child(div, span);
        doc.append_text(span, "Hello Megi");

        let mut styles = HashMap::new();
        styles.insert(
            html,
            make_box_style(
                Length::Auto,
                Length::Auto,
                Color::TRANSPARENT,
                0.0,
                Color::TRANSPARENT,
            ),
        );
        styles.insert(
            body,
            make_box_style(
                Length::Auto,
                Length::Auto,
                Color::TRANSPARENT,
                0.0,
                Color::TRANSPARENT,
            ),
        );

        let div_style = make_box_style(
            Length::Px(200.0),
            Length::Px(100.0),
            Color::RED,
            4.0,
            Color::BLUE,
        );
        styles.insert(div, div_style);

        let mut span_style = ComputedStyle::initial();
        span_style.color = Color::new(0, 255, 0, 255); // Green text
        styles.insert(span, span_style);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let layout_tree = layout_document(&doc, &styles, viewport).expect("layout tree");

        let display_list = build_display_list(&layout_tree, viewport);
        assert!(!display_list.is_empty(), "Display list should not be empty");

        // Verify that SolidColor (red bg), Border (blue), and Text ("Hello Megi") exist in display list
        let mut has_red_bg = false;
        let mut has_blue_border = false;
        let mut has_text = false;

        for item in &display_list {
            match item {
                DisplayItem::SolidColor { color, .. } if *color == Color::RED => {
                    has_red_bg = true;
                }
                DisplayItem::Border { color, .. } if *color == Color::BLUE => {
                    has_blue_border = true;
                }
                DisplayItem::Text { text, color, .. }
                    if text == "Hello Megi" && *color == Color::new(0, 255, 0, 255) =>
                {
                    has_text = true;
                }
                _ => {}
            }
        }

        assert!(has_red_bg, "Expected red background DisplayItem");
        assert!(has_blue_border, "Expected blue border DisplayItem");
        assert!(has_text, "Expected text DisplayItem");
    }

    #[test]
    fn test_pixmap_fill_rect_and_alpha_blending() {
        let mut pixmap = Pixmap::new(100, 100);

        // Fill background with opaque white
        pixmap.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0), Color::WHITE);
        assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
        assert_eq!(pixmap.pixel(50, 50), Color::WHITE);

        // Fill solid red rectangle at [10, 10, 20, 20]
        let red = Color::new(255, 0, 0, 255);
        pixmap.fill_rect(Rect::new(10.0, 10.0, 20.0, 20.0), red);
        assert_eq!(pixmap.pixel(15, 15), red);
        assert_eq!(pixmap.pixel(5, 5), Color::WHITE); // Unchanged

        // Fill semi-transparent black over a portion
        let semi_black = Color::new(0, 0, 0, 128);
        pixmap.fill_rect(Rect::new(10.0, 10.0, 10.0, 10.0), semi_black);
        let blended = pixmap.pixel(15, 15);
        // Red (255, 0, 0) blended with semi-transparent black (0, 0, 0, 128)
        assert!(blended.r > 100 && blended.r < 150);
        assert_eq!(blended.g, 0);
        assert_eq!(blended.b, 0);
        assert_eq!(blended.a, 255);
    }

    #[test]
    fn test_ppm_export_produces_valid_binary_p6() {
        let mut pixmap = Pixmap::new(4, 2);
        pixmap.fill_rect(Rect::new(0.0, 0.0, 2.0, 2.0), Color::RED);
        pixmap.fill_rect(Rect::new(2.0, 0.0, 2.0, 2.0), Color::BLUE);

        let ppm = pixmap.to_ppm();
        let expected_header = b"P6\n4 2\n255\n";
        assert!(ppm.starts_with(expected_header));

        // 4 x 2 pixels x 3 bytes (RGB) = 24 bytes of pixel data
        assert_eq!(ppm.len(), expected_header.len() + 24);
    }

    #[test]
    fn test_display_list_rasterization_end_to_end() {
        let mut dl = DisplayList::new();
        // Background: 100x100 white
        dl.push(DisplayItem::SolidColor {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            color: Color::WHITE,
        });
        // Box: 50x50 red at [10, 10]
        dl.push(DisplayItem::SolidColor {
            rect: Rect::new(10.0, 10.0, 50.0, 50.0),
            color: Color::RED,
        });
        // Text: 'A' at [20, 20] in blue
        dl.push(DisplayItem::Text {
            text: "A".to_string(),
            rect: Rect::new(20.0, 20.0, 20.0, 20.0),
            color: Color::BLUE,
            font_size: 16.0,
            font_family: "monospace".to_string(),
            advance: 20.0,
        });

        let pixmap = rasterize_display_list(&dl, 100, 100);

        // Outside box -> white
        assert_eq!(pixmap.pixel(0, 0), Color::WHITE);
        // Inside box, outside text -> red
        assert_eq!(pixmap.pixel(12, 12), Color::RED);
        // Inside 'A' glyph (first non-empty glyph cell row) -> blue text pixel
        assert_eq!(pixmap.pixel(26, 20), Color::BLUE);
    }
}
