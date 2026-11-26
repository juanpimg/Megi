//! Regression for the "Æuro-Office" Hacker News capture.
//!
//! The apparent mojibake is not a charset bug: `body_text` is correct and the triangle is
//! painted from `.votearrow`'s `triangle.svg` background. The visual defect is that the
//! 10×10 arrow is laid out inside a 4px-wide table cell and bleeds over the first letter of
//! the title, so the grey triangle plus "E" reads as "Æ" (and "I" as "Ä"). This fixture
//! reproduces the exact HN row markup and CSS declarations.

use std::path::Path;

use megi_core::geometry::Rect;
use megi_display::DisplayItem;
use megi_render::{RenderOptions, RenderOutput, Viewport, render_url};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/render")
        .join(name)
}

fn fixture_url(name: &str) -> String {
    format!("file://{}", fixture_path(name).display())
}

fn render_hn_titles() -> RenderOutput {
    let options = RenderOptions {
        viewport: Viewport::new(1024, 768).expect("viewport"),
        ..RenderOptions::default()
    };
    render_url(&fixture_url("hn_title_mojibake.html"), &options).expect("render HN titles")
}

fn title_text_runs(output: &RenderOutput) -> Vec<(String, Rect)> {
    output
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, rect, .. } => Some((text.clone(), *rect)),
            _ => None,
        })
        .collect()
}

fn triangle_rects(output: &RenderOutput) -> Vec<Rect> {
    output
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, .. }
                if (rect.width - 10.0).abs() < 1.0 && (rect.height - 10.0).abs() < 1.0 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn hn_titles_decode_as_text_without_mojibake() {
    let output = render_hn_titles();
    let texts: String = title_text_runs(&output)
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert!(
        texts.contains("Euro-Office"),
        "expected HN title: {texts:?}"
    );
    assert!(texts.contains("1Password"), "expected HN title: {texts:?}");
    assert!(
        !texts.contains('Æ') && !texts.contains('Ä'),
        "titles must not contain Latin-1 mojibake: {texts:?}"
    );
}

#[test]
fn hn_upvote_arrows_do_not_overlap_the_first_title_letter() {
    let output = render_hn_titles();
    let mut titles: Vec<(String, Rect)> = title_text_runs(&output)
        .into_iter()
        .filter(|(text, _)| text.starts_with("Euro-Office") || text.starts_with("1Password"))
        .collect();
    let mut arrows = triangle_rects(&output);
    titles.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    arrows.sort_by(|a, b| a.y.total_cmp(&b.y));
    assert_eq!(titles.len(), 2, "two HN titles must be painted");
    assert_eq!(
        arrows.len(),
        2,
        "both .votearrow backgrounds must be painted"
    );

    for ((title, rect), arrow) in titles.iter().zip(arrows.iter()) {
        let arrow_right = arrow.x + arrow.width;
        assert!(
            arrow_right <= rect.x,
            "{title:?}: grey arrow ends at x={arrow_right:.2}, title starts at x={:.2}; \
             the arrow must stay in its own table cell",
            rect.x
        );
    }
}

#[test]
fn hn_titles_png_is_byte_identical() {
    let output = render_hn_titles();
    let golden = std::fs::read(fixture_path("hn_title_mojibake.png")).expect("golden PNG");
    assert_eq!(
        output.png, golden,
        "the HN title/arrow painting changed; review the visual diff before updating the golden"
    );
}
