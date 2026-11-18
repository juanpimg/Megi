//! Dynamic state pseudo-classes (`:focus`, `:hover`, `:checked`, ...).

use crate::common::{append, computed, document, style_of};
use megi_css::{StateContext, matches_with_state, with_state};

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
        megi_css::Color {
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
        megi_css::Color {
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
