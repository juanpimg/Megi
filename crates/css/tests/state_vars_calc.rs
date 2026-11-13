//! Custom properties (`var()`), `calc()` and dynamic state pseudo-classes.

use std::collections::HashMap;

use css::{
    CalcContext, StateContext, evaluate_calc, matches_with_state, resolve_document_styles,
    with_calc_viewport, with_state,
};
use dom::{Attribute, Document, Namespace, NodeId};

fn append(doc: &mut Document, parent: NodeId, name: &str, attrs: &[(&str, &str)]) -> NodeId {
    let attributes = attrs
        .iter()
        .map(|(key, value)| Attribute::new(*key, *value))
        .collect();
    let id = doc.create_element(Namespace::Html, name, attributes);
    doc.append_child(parent, id);
    id
}

fn document() -> (Document, NodeId) {
    let mut doc = Document::new();
    let root = doc.root();
    let html = append(&mut doc, root, "html", &[]);
    let body = append(&mut doc, html, "body", &[]);
    (doc, body)
}

fn computed(doc: &Document, css_text: &str) -> HashMap<NodeId, css::ComputedStyle> {
    resolve_document_styles(doc, Some(css_text))
}

fn find_id(doc: &Document, id: &str) -> NodeId {
    doc.descendants(doc.root())
        .find(|node| {
            doc.node(*node)
                .as_element()
                .and_then(|element| element.get_attribute("id"))
                == Some(id)
        })
        .expect("element id")
}

fn style_of<'a>(
    styles: &'a HashMap<NodeId, css::ComputedStyle>,
    doc: &Document,
    id: &str,
) -> &'a css::ComputedStyle {
    styles.get(&find_id(doc, id)).expect("computed style")
}

// ---------------------------------------------------------------------------
// State pseudo-classes
// ---------------------------------------------------------------------------

#[test]
fn focus_and_focus_visible_from_attributes_and_context() {
    let (mut doc, body) = document();
    let input = append(
        &mut doc,
        body,
        "input",
        &[("id", "i"), ("data-megi-focused", "")],
    );
    assert!(matches_with_state(
        &doc,
        input,
        ":focus",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        input,
        "input:focus",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        input,
        ":focus-visible",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        input,
        ":hover",
        &StateContext::default()
    ));

    let other = append(&mut doc, body, "input", &[("id", "j")]);
    let context = StateContext {
        focused: Some(other),
        focus_visible: Some(other),
        ..StateContext::new()
    };
    assert!(matches_with_state(&doc, other, ":focus", &context));
    assert!(!matches_with_state(&doc, input, ":focus", &context));
    assert!(matches_with_state(
        &doc,
        other,
        "input:focus-visible",
        &context
    ));
}

#[test]
fn hover_and_active_are_host_mapped() {
    let (mut doc, body) = document();
    let div = append(
        &mut doc,
        body,
        "div",
        &[("id", "d"), ("data-megi-hover", "")],
    );
    assert!(matches_with_state(
        &doc,
        div,
        "div:hover",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        div,
        ":active",
        &StateContext::default()
    ));
    let context = StateContext {
        hovered: Some(div),
        active: Some(div),
        ..StateContext::new()
    };
    assert!(matches_with_state(&doc, div, ":hover", &context));
    assert!(matches_with_state(&doc, div, "div:active", &context));
}

#[test]
fn checked_not_and_compound_selectors() {
    let (mut doc, body) = document();
    let checked = append(
        &mut doc,
        body,
        "input",
        &[("type", "checkbox"), ("checked", "")],
    );
    let unchecked = append(&mut doc, body, "input", &[("type", "checkbox")]);
    assert!(matches_with_state(
        &doc,
        checked,
        "input:checked",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        checked,
        "input[type='checkbox']:checked",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        unchecked,
        "input:not(:checked)",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        unchecked,
        ":checked",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        checked,
        ":checked, :disabled",
        &StateContext::default()
    ));
}

#[test]
fn disabled_enabled_and_fieldset_inheritance() {
    let (mut doc, body) = document();
    let disabled = append(&mut doc, body, "input", &[("disabled", "")]);
    let enabled = append(&mut doc, body, "input", &[("id", "e")]);
    let fieldset = append(&mut doc, body, "fieldset", &[("disabled", "")]);
    let nested = append(&mut doc, fieldset, "input", &[]);
    assert!(matches_with_state(
        &doc,
        disabled,
        "input:disabled",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        enabled,
        "input:enabled",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        nested,
        ":disabled",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        enabled,
        ":disabled",
        &StateContext::default()
    ));
}

#[test]
fn read_only_read_write_required_placeholder() {
    let (mut doc, body) = document();
    let read_only = append(&mut doc, body, "input", &[("readonly", "")]);
    let read_write = append(&mut doc, body, "input", &[("id", "rw")]);
    let div = append(&mut doc, body, "div", &[]);
    let required = append(&mut doc, body, "input", &[("required", "")]);
    let placeholder = append(&mut doc, body, "input", &[("placeholder", "name")]);
    let filled = append(
        &mut doc,
        body,
        "input",
        &[("placeholder", "n"), ("value", "x")],
    );
    assert!(matches_with_state(
        &doc,
        read_only,
        ":read-only",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        read_write,
        ":read-write",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        div,
        ":read-only",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        required,
        ":required",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        placeholder,
        ":placeholder-shown",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        filled,
        ":placeholder-shown",
        &StateContext::default()
    ));
}

#[test]
fn state_pseudo_classes_drive_the_cascade() {
    let (mut doc, body) = document();
    let input = append(&mut doc, body, "input", &[("id", "i")]);
    let css = "input { color: #000000 } input:focus { color: #ff0000 }";
    let plain = computed(&doc, css);
    assert_eq!(
        style_of(&plain, &doc, "i").color,
        css::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
    let focused = StateContext {
        focused: Some(input),
        ..StateContext::new()
    };
    let styles = with_state(focused, || computed(&doc, css));
    assert_eq!(
        style_of(&styles, &doc, "i").color,
        css::Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn state_pseudo_classes_work_in_list_and_not() {
    let (mut doc, body) = document();
    let a = append(&mut doc, body, "input", &[("checked", "")]);
    let b = append(&mut doc, body, "input", &[]);
    assert!(matches_with_state(
        &doc,
        a,
        ":checked:not(:disabled)",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        b,
        ":checked:not(:disabled)",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        b,
        ":enabled, :checked",
        &StateContext::default()
    ));
}

// ---------------------------------------------------------------------------
// Custom properties / var()
// ---------------------------------------------------------------------------

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
        css::Color {
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
        css::Color {
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
        css::Color {
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
        css::Color {
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
        css::Color::BLACK,
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
    assert_eq!(style.width, css::Length::Px(20.0));
    assert_eq!(style.margin.top, css::Length::Px(10.0));
    assert_eq!(style.margin.left, css::Length::Px(10.0));
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
    assert_eq!(style.margin.top, css::Length::Px(4.0));
    assert_eq!(style.margin.right, css::Length::Px(4.0));
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
        css::Color {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 255
        }
    );
}

// ---------------------------------------------------------------------------
// calc()
// ---------------------------------------------------------------------------

#[test]
fn calc_absolute_arithmetic() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(
        &doc,
        "div { margin: calc(10px + 2 * 5px); width: calc(100px / 4); height: calc((10px + 5px) * 2) }",
    );
    let style = style_of(&styles, &doc, "d");
    assert_eq!(style.margin.top, css::Length::Px(20.0));
    assert_eq!(style.width, css::Length::Px(25.0));
    assert_eq!(style.height, css::Length::Px(30.0));
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
    assert_eq!(style.margin.top, css::Length::Px(22.0));
    assert_eq!(style.width, css::Length::Px(32.0));
    assert_eq!(style.height, css::Length::Percent(50.0));
}

#[test]
fn calc_vw_vh_use_the_injected_viewport() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = with_calc_viewport(1000.0, 500.0, || {
        computed(&doc, "div { width: calc(10vw + 2vh) }")
    });
    assert_eq!(style_of(&styles, &doc, "d").width, css::Length::Px(110.0));
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
    assert_eq!(style.width, css::Length::Px(-20.0));
    // Invalid expression → initial `auto` and no calc components.
    assert_eq!(style.height, css::Length::Auto);
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
        css::Length::Px(6.0)
    );
}

#[test]
fn root_and_empty_structural_pseudo_classes_still_work() {
    let (mut doc, body) = document();
    let html = doc.node(body).parent.expect("html");
    let empty_div = append(&mut doc, body, "div", &[("id", "empty")]);
    assert!(matches_with_state(
        &doc,
        html,
        ":root",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        empty_div,
        ":root",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        empty_div,
        ":empty",
        &StateContext::default()
    ));
    assert!(matches_with_state(
        &doc,
        empty_div,
        "body > div:not(:root)",
        &StateContext::default()
    ));
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
        css::Color {
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
        css::Color {
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
        css::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
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
    assert_eq!(style.margin.top, css::Length::Px(-10.0));
    assert!((style.padding.top.top_px() - 4.5).abs() < 0.001);
}

#[test]
fn calc_in_background_position() {
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(&doc, "div { background-position: calc(10px + 5px) 20px }");
    let style = style_of(&styles, &doc, "d");
    let position = &style.background_position;
    assert!(
        format!("{position:?}").contains("15"),
        "background-position: {position:?}"
    );
}

#[test]
fn data_megi_disabled_maps_to_disabled_pseudo_class() {
    let (mut doc, body) = document();
    let button = append(
        &mut doc,
        body,
        "button",
        &[("id", "b"), ("data-megi-disabled", "true")],
    );
    assert!(matches_with_state(
        &doc,
        button,
        "button:disabled",
        &StateContext::default()
    ));
    assert!(!matches_with_state(
        &doc,
        button,
        "button:enabled",
        &StateContext::default()
    ));
}

// Helper used by `calc_negative_and_integer_division`.
trait LengthPx {
    fn top_px(&self) -> f32;
}
impl LengthPx for css::Length {
    fn top_px(&self) -> f32 {
        match self {
            css::Length::Px(value) => *value,
            _ => f32::NAN,
        }
    }
}

// ---------------------------------------------------------------------------
// Class matching is case-sensitive (#682)
// ---------------------------------------------------------------------------

#[test]
fn class_selectors_are_case_sensitive_in_the_cascade() {
    // Selectors 4: only type selectors get ASCII-insensitive treatment for HTML
    // elements; `.Foo` must not match `class="foo"`.
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "lower"), ("class", "foo")]);
    append(&mut doc, body, "div", &[("id", "upper"), ("class", "Foo")]);
    let css = ".Foo { color: #ff0000 } .foo { color: #0000ff }";
    let styles = computed(&doc, css);
    assert_eq!(
        style_of(&styles, &doc, "lower").color,
        css::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        },
        "`.foo` must apply to class=\"foo\" and `.Foo` must not"
    );
    assert_eq!(
        style_of(&styles, &doc, "upper").color,
        css::Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        },
        "`.Foo` must apply to class=\"Foo\" and `.foo` must not"
    );
}

#[test]
fn class_matching_does_not_fold_case_when_the_index_is_used() {
    // Same rule reached through the class index: the bucket key keeps the selector's
    // spelling, so the candidate filter cannot smuggle in a folded match.
    let (mut doc, body) = document();
    append(
        &mut doc,
        body,
        "p",
        &[("id", "up"), ("class", "Button Primary")],
    );
    append(
        &mut doc,
        body,
        "p",
        &[("id", "down"), ("class", "button primary")],
    );
    let styles = computed(&doc, ".Button { color: #00ff00 }");
    let green = css::Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    assert_eq!(
        style_of(&styles, &doc, "up").color,
        green,
        "class=\"Button Primary\" contains the exact class `.Button`"
    );
    assert_ne!(
        style_of(&styles, &doc, "down").color,
        green,
        "class=\"button primary\" must not match `.Button`"
    );
}

#[test]
fn type_selectors_stay_ascii_case_insensitive_for_html() {
    // Guard against over-correcting: `DIV` still matches <div> (type selector rules).
    let (mut doc, body) = document();
    append(&mut doc, body, "div", &[("id", "d")]);
    let styles = computed(&doc, "DIV { color: #123456 }");
    assert_eq!(
        style_of(&styles, &doc, "d").color,
        css::Color {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 255
        }
    );
}
