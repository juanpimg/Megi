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
    let _ = sheet.rules.len();
}

#[test]
fn normal_nesting_still_parses() {
    let css = format!("a{{width:calc({}1px{})}}", "(".repeat(8), ")".repeat(8));
    let sheet = parse_stylesheet(&css);
    assert_eq!(sheet.rules.len(), 1);
}
