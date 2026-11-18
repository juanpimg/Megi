//! `calc()` arithmetic and the public evaluator API.

use crate::common::{append, computed, document, style_of};
use megi_css::{CalcContext, evaluate_calc, with_calc_viewport};

#[test]
fn calc_absolute_arithmetic() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { margin: calc(10px + 2 * 5px); width: calc(100px / 4); height: calc((10px + 5px) * 2) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.margin.top, megi_css::Length::Px(20.0));
    assert_eq!(style.width, megi_css::Length::Px(25.0));
    assert_eq!(style.height, megi_css::Length::Px(30.0));
}

#[test]
fn calc_em_rem_and_percent() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { font-size: 20px; margin: calc(1em + 2px); width: calc(2rem); height: calc(50%) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.margin.top, megi_css::Length::Px(22.0));
    assert_eq!(style.width, megi_css::Length::Px(32.0));
    assert_eq!(style.height, megi_css::Length::Percent(50.0));
}

#[test]
fn calc_vw_vh_use_the_injected_viewport() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = with_calc_viewport(1000.0, 500.0, || {
        computed(&doc, "div { width: calc(10vw + 2vh) }")
    });
    assert_eq!(
        style_of(&styles, &doc, "d").width,
        megi_css::Length::Px(110.0)
    );
}

#[test]
fn calc_mixed_stores_components_and_invalid_falls_back_to_auto() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { width: calc(100% - 20px); height: calc(10px +) }",
    );
    let style = style_of(&styles, &doc, "d");
    // Mixed percentage+length is supported since R119 (task-122): the components are kept
    // in `calc_lengths` and the `Length` fallback keeps the absolute part for legacy users.
    let width = style.calc_lengths.width.expect("mixed width components");
    assert_eq!(width.percent, 100.0);
    assert_eq!(width.px, -20.0);
    assert_eq!(style.width, megi_css::Length::Px(-20.0));
    // Invalid expression → initial `auto` and no calc components.
    assert_eq!(style.height, megi_css::Length::Auto);
    assert!(style.calc_lengths.height.is_none());
}

#[test]
fn calc_public_evaluator_api() {
    let context = CalcContext {
        font_size: 20.0,
        root_font_size: 16.0,
        viewport_width: 1000.0,
        viewport_height: 500.0,
    };
    let value = evaluate_calc("calc(1em + 10px)", &context).expect("valid calc");
    assert_eq!(value.px, 30.0);
    assert_eq!(value.percent, 0.0);
    assert!(evaluate_calc("calc(100% - 10px)", &context).is_some());
    assert!(evaluate_calc("calc(1px +)", &context).is_none());
    assert!(evaluate_calc("calc(1em * 2em)", &context).is_none());
    assert!(evaluate_calc("10px", &context).is_none());
}

#[test]
fn calc_in_font_size_and_border_width() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { font-size: calc(10px + 6px); border-width: calc(1px + 2px); border-style: solid }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.border_width.top, 3.0);
}

#[test]
fn calc_and_var_combined() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --pad: 2px } div { padding: calc(var(--pad) * 3) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "d").padding.top,
        megi_css::Length::Px(6.0)
    );
}

#[test]
fn calc_negative_and_integer_division() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { margin: calc(10px - 20px); padding: calc(9px / 2) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.margin.top, megi_css::Length::Px(-10.0));
    assert!((style.padding.top.top_px() - 4.5).abs() < 0.001);
}

#[test]
fn calc_in_background_position() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(&doc, "div { background-position: calc(10px + 5px) 20px }");
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.background_position.x, megi_css::Length::Px(15.0));
    assert_eq!(style.background_position.y, megi_css::Length::Px(20.0));
}

// Helper used by `calc_negative_and_integer_division`.
trait LengthPx {
    fn top_px(&self) -> f32;
}
impl LengthPx for megi_css::Length {
    fn top_px(&self) -> f32 {
        match self {
            megi_css::Length::Px(value) => *value,
            _ => f32::NAN,
        }
    }
}
