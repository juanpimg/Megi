//! Component-value nesting is bounded: the grammar is recursive (§5.5.8-§5.5.10) and a
//! ~100 KB stylesheet of `(` used to overflow the host stack before any declaration was
//! validated.

use css::parse_stylesheet;

#[test]
fn deeply_nested_parentheses_do_not_overflow_the_stack() {
    let css = format!("a{{color:{}", "(".repeat(100_000));
    let sheet = parse_stylesheet(&css);
    assert!(sheet.rules.len() <= 1);
    assert!(sheet.imports.is_empty());
}

#[test]
fn deeply_nested_functions_do_not_overflow_the_stack() {
    let css = format!(
        "a{{color:{}red{}}}",
        "rgb(".repeat(50_000),
        ")".repeat(50_000)
    );
    let sheet = parse_stylesheet(&css);
    assert!(sheet.rules.len() <= 1);
}

#[test]
fn deeply_nested_selectors_do_not_overflow_the_stack() {
    let css = format!("{}a{{}}", ":is(".repeat(50_000));
    let sheet = parse_stylesheet(&css);
    // Past `MAX_SELECTOR_NESTING` the selector list is invalid, so the rule is dropped (#1356).
    assert!(
        sheet.rules.is_empty(),
        "over-nested selector must not produce a rule"
    );
    assert!(sheet.imports.is_empty());
    assert!(sheet.font_faces.is_empty());
    assert!(sheet.keyframes.is_empty());
}

#[test]
fn balanced_selector_nesting_is_capped_not_recursed() {
    // The unbalanced case above bails out on the first level; balanced arguments are the
    // ones that actually recursed through `parse_selectors` (#376).
    let depth = 20_000;
    let css = format!("{}a{}{{}}", ":is(".repeat(depth), ")".repeat(depth));
    let sheet = parse_stylesheet(&css);
    // Past `MAX_SELECTOR_NESTING` the selector list is invalid, so nothing is applied.
    assert!(sheet.rules.len() <= 1);
}

#[test]
fn selector_nesting_below_the_cap_still_parses() {
    let depth = css::MAX_SELECTOR_NESTING.min(8);
    let css = format!(
        "{}a{}{{color:red}}",
        ":is(".repeat(depth),
        ")".repeat(depth)
    );
    let sheet = parse_stylesheet(&css);
    assert_eq!(sheet.rules.len(), 1);
}

#[test]
fn normal_nesting_still_parses() {
    let css = format!("a{{width:calc({}1px{})}}", "(".repeat(8), ")".repeat(8));
    let sheet = parse_stylesheet(&css);
    assert_eq!(sheet.rules.len(), 1);
}
