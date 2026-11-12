//! The in-SVG `<style>` cascade is bounded: rules × selectors × elements can otherwise be
//! made arbitrarily large from a small document (a remote CPU DoS).

use svg::{Limits, SvgError, SvgOptions, render_svg_with_limits};

#[test]
fn stylesheet_selector_checks_are_budgeted() {
    // 21 elements × 1 rule × 1 selector is far above a budget of 5: the cascade must stop
    // with a typed error instead of evaluating every rule against every element.
    let mut body = String::new();
    for _ in 0..20 {
        body.push_str("<rect width='1' height='1'/>");
    }
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
         <style>rect{{fill:red}}</style>{body}</svg>"
    );
    let limits = Limits {
        max_selector_checks: 5,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("selector budget exceeded");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}
