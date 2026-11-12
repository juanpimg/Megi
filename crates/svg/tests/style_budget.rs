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

#[test]
fn clip_builders_share_the_selector_budget() {
    // Each `<clipPath>` content is flattened by its own Builder. With a per-builder budget
    // each one would pass, and a page could stack thousands of them under one stylesheet;
    // the budget must be shared across the whole document.
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
        <style>.a{fill:red}</style>\
        <defs>\
          <clipPath id='c1'><rect class='a' width='4' height='4'/></clipPath>\
          <clipPath id='c2'><rect class='a' width='4' height='4'/></clipPath>\
        </defs>\
        <rect width='8' height='8' clip-path='url(#c1)'/></svg>";
    let limits = Limits {
        max_selector_checks: 4,
        ..Limits::default()
    };
    let error = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect_err("clip builders must charge the shared budget");
    assert!(matches!(error, SvgError::Limit(_)), "{error:?}");
}

#[test]
fn a_below_budget_stylesheet_still_renders() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>\
        <style>.a{fill:red}</style><rect class='a' width='8' height='8'/></svg>";
    let limits = Limits {
        max_selector_checks: usize::MAX,
        ..Limits::default()
    };
    let image = render_svg_with_limits(source.as_bytes(), &SvgOptions::default(), &limits)
        .expect("an unlimited budget renders");
    assert_eq!((image.width, image.height), (8, 8));
}
