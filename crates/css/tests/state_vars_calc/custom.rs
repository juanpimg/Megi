//! Custom properties and `var()` substitution.

use crate::common::{append, computed, document, style_of};

#[test]
fn root_custom_property_is_inherited() {
    let (mut doc, body) = document();
    let div = append(&mut doc, body, "div", &[("id", "d")]);
    let _ = div;
    let styles = computed(
        &doc,
        ":root { --brand: #ff0000 } body { color: var(--brand) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        megi_css::Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn element_override_and_initial_reset() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "parent")]);
    let styles = computed(
        &doc,
        ":root { --c: #00ff00 } #parent { --c: #0000ff } div { color: var(--c) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "parent").color,
        megi_css::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
    let (mut doc2, body2) = document();
    append(&mut doc2, body2, "div", &[("id", "reset")]);
    let styles2 = computed(
        &doc2,
        ":root { --c: #00ff00 } #reset { --c: initial } div { color: var(--c, #000000) }",
    );
    assert_eq!(
        style_of(&styles2, &doc2, "reset").color,
        megi_css::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn var_fallback_cycle_and_undefined() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --a: var(--a) } div { color: var(--a, #abcdef) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        megi_css::Color {
            r: 0xab,
            g: 0xcd,
            b: 0xef,
            a: 255
        }
    );
    let (mut doc2, body2) = document();
    append(&mut doc2, body2, "div", &[("id", "d")]);
    let styles2 = computed(&doc2, "div { color: var(--missing) }");
    assert_eq!(
        style_of(&styles2, &doc2, "d").color,
        megi_css::Color::BLACK,
        "undefined var without fallback is invalid at computed-value time"
    );
}

#[test]
fn nested_variables_and_lengths() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --base: 10px; --double: calc(var(--base) * 2) } div { width: var(--double); margin: var(--base) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.width, megi_css::Length::Px(20.0));
    assert_eq!(style.margin.top, megi_css::Length::Px(10.0));
    assert_eq!(style.margin.left, megi_css::Length::Px(10.0));
}

#[test]
fn var_in_shorthand_and_font_family() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --spacing: 4px; --fonts: 'DejaVu Sans', sans-serif } \
         div { margin: var(--spacing) var(--spacing); font-family: var(--fonts) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.margin.top, megi_css::Length::Px(4.0));
    assert_eq!(style.margin.right, megi_css::Length::Px(4.0));
    assert!(
        style.font_family.contains("DejaVu Sans"),
        "{}",
        style.font_family
    );
}

#[test]
fn var_in_inline_style() {
    let (mut doc, body) = document();
    let div = append(
        &mut doc,
        body,
        "div",
        &[("id", "d"), ("style", "color: var(--inline, #123456)")],
    );
    let _ = div;
    let styles = computed(&doc, "div { color: #000000 }");
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        megi_css::Color {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 255
        }
    );
}

#[test]
fn custom_property_names_are_case_sensitive() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --Color: #ff0000; --color: #00ff00 } div { color: var(--color) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        megi_css::Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255
        }
    );
    let (mut doc2, body2) = document();
    append(&mut doc2, body2, "div", &[("id", "d")]);
    let styles2 = computed(
        &doc2,
        ":root { --Color: #ff0000 } div { color: var(--Color) }",
    );
    assert_eq!(
        style_of(&styles2, &doc2, "d").color,
        megi_css::Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn var_fallback_can_reference_another_variable() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        ":root { --backup: #0000ff } div { color: var(--missing, var(--backup)) }",
    );
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        megi_css::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
}
