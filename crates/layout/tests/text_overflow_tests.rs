//! #1929 — `text-overflow: ellipsis` in the inline formatting context.
//!
//! Regression tests for the two rules the original implementation got wrong:
//! CSS UI 3 §4.1 restricts `text-overflow` to clipping containers, and the
//! ellipsis advance must come from the font engine (`TextMeasurer`).

use std::collections::HashMap;

use megi_core::{Document, Namespace};
use megi_css::{ComputedStyle, Display, Length, Overflow, TextOverflow, WhiteSpace};
use megi_layout::{
    BoxType, FontMetrics, Rect, TextMeasurer, layout_document, layout_document_with_measurer,
};

fn make_block_style(width: Length, height: Length) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.display = Display::Block;
    style.width = width;
    style.height = height;
    style
}

fn find_text(node: &megi_layout::LayoutBox) -> Option<(&str, f32)> {
    if let BoxType::TextNode(text) = &node.box_type {
        return Some((text, node.dimensions.content.width));
    }
    node.children.iter().find_map(find_text)
}

fn ellipsis_document(text: &str) -> (Document, Vec<megi_core::NodeId>) {
    let mut doc = Document::new();
    let html = doc.create_element(Namespace::Html, "html", vec![]);
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let div = doc.create_element(Namespace::Html, "div", vec![]);
    doc.append_child(doc.root(), html);
    doc.append_child(html, body);
    doc.append_child(body, div);
    doc.append_text(div, text);
    (doc, vec![html, body, div])
}

fn styles_for(
    ids: &[megi_core::NodeId],
    div_style: ComputedStyle,
) -> HashMap<megi_core::NodeId, ComputedStyle> {
    let mut styles = HashMap::new();
    styles.insert(ids[0], make_block_style(Length::Auto, Length::Auto));
    styles.insert(ids[1], make_block_style(Length::Auto, Length::Auto));
    styles.insert(ids[2], div_style);
    styles
}

#[test]
fn ellipsis_is_ignored_when_overflow_is_visible() {
    let (doc, ids) = ellipsis_document("ABCDEFGHIJKLMNOP");
    let mut div_style = make_block_style(Length::Px(60.0), Length::Auto);
    div_style.font_size = 16.0;
    div_style.white_space = WhiteSpace::Nowrap;
    div_style.overflow = Overflow::Visible;
    div_style.text_overflow = TextOverflow::Ellipsis;

    let styles = styles_for(&ids, div_style);
    let tree = layout_document(&doc, &styles, Rect::new(0.0, 0.0, 200.0, 60.0)).expect("layout");
    let (text, width) = find_text(&tree).expect("text box");
    assert_eq!(text, "ABCDEFGHIJKLMNOP", "no clipping means no ellipsis");
    assert_eq!(width, 128.0, "default measurer: 16 chars * 8px");
}

#[test]
fn ellipsis_width_is_measured_with_the_font_engine() {
    struct WideEllipsisMeasurer;
    impl TextMeasurer for WideEllipsisMeasurer {
        fn measure_text(&self, text: &str, family: &str, size: f32) -> f32 {
            self.measure_text_styled(text, family, size, 400, false)
        }
        fn font_metrics(&self, _family: &str, size: f32, line_height: Option<f32>) -> FontMetrics {
            FontMetrics::new(size, line_height)
        }
        fn measure_text_styled(
            &self,
            text: &str,
            _family: &str,
            _size: f32,
            _weight: u16,
            _italic: bool,
        ) -> f32 {
            text.chars()
                .map(|ch| if ch == '…' { 16.0 } else { 10.0 })
                .sum()
        }
    }

    let (doc, ids) = ellipsis_document("AAAAAAAAAAAA");
    let mut div_style = make_block_style(Length::Px(60.0), Length::Auto);
    div_style.font_size = 16.0;
    div_style.white_space = WhiteSpace::Nowrap;
    div_style.overflow = Overflow::Hidden;
    div_style.text_overflow = TextOverflow::Ellipsis;

    let styles = styles_for(&ids, div_style);
    let measurer = WideEllipsisMeasurer;
    let tree =
        layout_document_with_measurer(&doc, &styles, Rect::new(0.0, 0.0, 200.0, 60.0), &measurer)
            .expect("layout");
    let (text, width) = find_text(&tree).expect("text box");
    // 60px box, 16px ellipsis => 44px for glyphs at 10px each => 4 chars.
    assert_eq!(
        text, "AAAA…",
        "ellipsis width must be measured, not guessed"
    );
    assert_eq!(width, 56.0, "run width must be measured, not evenly split");
}
