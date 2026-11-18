//! #1523: Intl.NumberFormat and Intl.DateTimeFormat subsets — locale-aware grouping, fraction digits and date order.

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

#[test]
fn datetime_format_and_to_locale_date_string_use_locale_order() {
    let out = eval(
        "var d = new Date(2024, 0, 15, 12); \
         var us = new Intl.DateTimeFormat('en-US').format(d); \
         var es = new Intl.DateTimeFormat('es-ES').format(d); \
         String(us + '|' + es + '|' + d.toLocaleDateString('en-GB'));",
    );
    assert_eq!(out, "01/15/2024|15/01/2024|15/01/2024", "{out}");
}

#[test]
fn date_to_locale_time_and_date_time_strings() {
    let out = eval(
        "var d = new Date(2024, 0, 15, 9, 5, 7); \
         String(d.toLocaleTimeString('en-US') + '|' + d.toLocaleString('en-US') + '|' + d.toLocaleString('es-ES'));",
    );
    assert_eq!(
        out, "09:05:07|01/15/2024, 09:05:07|15/01/2024 09:05:07",
        "{out}"
    );
}

#[test]
fn datetime_format_time_only() {
    let out = eval(
        "var d = new Date(2024, 0, 15, 9, 5, 7); \
         var t = new Intl.DateTimeFormat('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit' }); \
         String(t.format(d));",
    );
    assert_eq!(out, "09:05:07", "{out}");
}
