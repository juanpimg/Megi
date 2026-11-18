//! Unit tests for the deterministic benchmark fixture generators.

use super::*;

#[test]
fn scaling_never_returns_zero() {
    assert_eq!(scale(0, 1.0), 1);
    assert_eq!(scale(10, 0.01), 1);
    assert_eq!(scale(10_000, 0.5), 5_000);
    assert_eq!(scale(10_000, 2.0), 20_000);
}

#[test]
fn generators_are_deterministic() {
    assert_eq!(nested_divs(64), nested_divs(64));
    assert_eq!(massive_end_p(32), massive_end_p(32));
    assert_eq!(massive_formatting(8), massive_formatting(8));
    assert_eq!(tag_with_attributes(16), tag_with_attributes(16));
    assert_eq!(pretty_printed(4), pretty_printed(4));
    assert_eq!(stylesheet_rules(8), stylesheet_rules(8));
    assert_eq!(element_document(8), element_document(8));
    assert_eq!(gradients_page(8), gradients_page(8));
    assert_eq!(grid_page(), grid_page());
    assert_eq!(vars_calc_css(), vars_calc_css());
    assert_eq!(clipping_page(8), clipping_page(8));
    assert_eq!(sticky_text_page(4096), sticky_text_page(4096));
}

#[test]
fn workloads_have_the_requested_shape() {
    let nested = nested_divs(100);
    assert_eq!(nested.matches("<div>").count(), 100);
    assert_eq!(nested.matches("</div>").count(), 100);
    assert!(nested.contains(">x<"));

    let ends = massive_end_p(50);
    assert_eq!(ends.matches("</p>").count(), 50);

    let attrs = tag_with_attributes(40);
    assert_eq!(attrs.matches("=\"").count(), 40);

    let pretty = pretty_printed(7);
    assert_eq!(pretty.matches("class=\"block\"").count(), 7);

    let rules = stylesheet_rules(11);
    assert_eq!(rules.matches(".rule-").count(), 11);

    let gradients = gradients_page(9);
    assert_eq!(gradients.matches("background-image").count(), 9);
    assert!(gradients.contains("linear-gradient("));
    assert!(gradients.contains("radial-gradient("));

    let grids = grid_page();
    assert_eq!(grids.matches("display:grid").count(), 3);
    assert!(grids.contains("repeat(auto-fill"));
    assert!(grids.contains("grid-column:span 2"));
    assert_eq!(grids.matches("class=\"cell").count(), GRID_CELLS);

    let vars = vars_calc_css();
    assert!(vars.contains(":root{"));
    assert!(vars.contains("var(--ink)"));
    assert!(vars.contains("calc("));

    let clip = clipping_page(6);
    assert_eq!(clip.matches("class=\"clip\"").count(), 6);
    assert!(clip.contains("border-radius:28px"));
    assert!(clip.contains("overflow:hidden"));
    assert!(clip.contains("background-clip:text"));

    let sticky = sticky_text_page(4096);
    assert!(sticky.len() >= 4096);
    assert!(sticky.contains("position:sticky"));
    assert!(sticky.contains("text-transform:uppercase"));
    assert!(sticky.contains("text-transform:lowercase"));
    assert!(sticky.contains("text-transform:capitalize"));
    assert!(sticky.ends_with("</html>\n"));
}

#[test]
fn realistic_document_reaches_the_target_size() {
    let document = realistic_document(64 * 1024);
    assert!(document.html.len() >= 64 * 1024);
    assert!(document.sections > 0);
    assert!(document.style_rules >= 128);
    assert!(document.html.starts_with("<!DOCTYPE html>"));
    assert!(document.html.ends_with("</html>\n"));
}
