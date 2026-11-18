//! #1523: Intl.NumberFormat subset — locale-aware grouping and fraction digits.

#![cfg(feature = "js")]

use js::runtime::JsRuntime;

fn eval(script: &str) -> String {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.eval(script, "t").expect("eval")
}

#[test]
fn number_format_groups_by_locale() {
    let out = eval(
        "var us = new Intl.NumberFormat('en-US'); \
         var es = new Intl.NumberFormat('es-ES'); \
         var de = new Intl.NumberFormat('de-DE'); \
         String(us.format(1234567.5) + '|' + es.format(1234567.5) + '|' + de.format(1234567.5));",
    );
    assert_eq!(
        out, "1,234,567.5|1.234.567,5|1.234.567,5",
        "grouping: {out}"
    );
}

#[test]
fn number_format_fraction_digits() {
    let out = eval(
        "var f = new Intl.NumberFormat('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 }); \
         String(f.format(42.1) + '|' + f.format(3) + '|' + f.format(-1234.56));",
    );
    assert_eq!(out, "42.10|3.00|-1,234.56", "fractions: {out}");
}

#[test]
fn number_format_no_grouping_and_locale_string() {
    let out = eval(
        "var f = new Intl.NumberFormat('en-US', { useGrouping: false }); \
         String(f.format(1000000) + '|' + (1234567.891).toLocaleString('en-US', { maximumFractionDigits: 1 }));",
    );
    assert_eq!(
        out, "1000000|1,234,567.9",
        "no-grouping + toLocaleString: {out}"
    );
}

#[test]
fn number_format_resolved_options_and_nonfinite() {
    let out = eval(
        "var f = new Intl.NumberFormat('fr-FR', { minimumFractionDigits: 1 }); \
         var o = f.resolvedOptions(); \
         String(o.locale + '|' + o.minimumFractionDigits + '|' + f.format(Infinity));",
    );
    assert_eq!(out, "fr-FR|1|Infinity", "resolved options: {out}");
}
