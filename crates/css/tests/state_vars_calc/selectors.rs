//! Selector matching case-sensitivity regressions (#682).

use crate::common::{append, computed, document, style_of};

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
        megi_css::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        },
        "`.foo` must apply to class=\"foo\" and `.Foo` must not"
    );
    assert_eq!(
        style_of(&styles, &doc, "upper").color,
        megi_css::Color {
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
    let green = megi_css::Color {
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
        megi_css::Color {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 255
        }
    );
}
