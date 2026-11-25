//! Hot-script workloads: pure engine time vs DOM-binding time (#M8#1).
//!
//! Both scripts come from the same real-site pattern in
//! `tests/fixtures/js_corpus/jquery_like.html`: `querySelectorAll('.item')`
//! followed by a hot loop of `getAttribute('data-index')`. The paired rows
//! differ only in the object behind `items[i]`: plain JS objects in
//! `js/hot-engine` (`engine_time_ms`) and real DOM wrappers in
//! `js/hot-bindings` (`binding_time_ms` + engine). The delta isolates the
//! Rust bridge; it is computed in `docs/PERFORMANCE.md`, never claimed as a
//! Boa/QuickJS comparison. A third row evaluates the unmodified inline
//! script of the committed `wikipedia_null` corpus fixture.

use super::*;

const ITEMS: usize = 200;
const REPS: usize = 100;
const CORPUS_HTML: &str = include_str!("../../../../tests/fixtures/js_corpus/wikipedia_null.html");

/// HTML with `items` real elements for the binding variant.
fn fixture_html(items: usize) -> String {
    let mut html = String::with_capacity(items * 48 + 128);
    html.push_str("<!doctype html><html><body>");
    for index in 0..items {
        use std::fmt::Write as _;
        let _ = write!(html, "<div class=\"item\" data-index=\"{index}\"></div>");
    }
    html.push_str("<div id=\"result\">pending</div></body></html>");
    html
}

/// One-off setup for the engine variant, evaluated before the timed window.
/// Keeping the shim allocation out of the loop is what makes the paired rows
/// comparable: both variants select their `items` collection per evaluation.
fn engine_setup_script(items: usize) -> String {
    format!(
        "var __shims = new Array({items});\
         for (var i = 0; i < {items}; i++) {{\
           __shims[i] = {{ getAttribute: function (name) {{ return name === 'data-index' ? 'x' : ''; }} }};\
         }}"
    )
}

/// Engine-only variant: identical loop and method calls on plain JS objects.
fn engine_script(items: usize, reps: usize) -> String {
    format!(
        "(function () {{\
           var items = __shims;\
           var acc = '';\
           for (var r = 0; r < {reps}; r++) {{\
             for (var i = 0; i < {items}; i++) {{ acc = items[i].getAttribute('data-index'); }}\
           }}\
           return acc;\
         }})()"
    )
}

/// DOM variant: same loop and call count, but each call crosses into Rust.
fn bindings_script(items: usize, reps: usize) -> String {
    format!(
        "(function () {{\
           var items = document.querySelectorAll('.item');\
           var acc = '';\
           for (var r = 0; r < {reps}; r++) {{\
             for (var i = 0; i < {items}; i++) {{ acc = items[i].getAttribute('data-index'); }}\
           }}\
           return acc;\
         }})()"
    )
}

/// Extracts the single inline script from a committed corpus fixture.
fn inline_script(html: &'static str) -> &'static str {
    let start = html
        .find("<script>")
        .expect("corpus fixture has an inline script")
        + "<script>".len();
    let rest = &html[start..];
    let end = rest
        .find("</script>")
        .expect("corpus fixture closes its inline script")
        + start;
    &html[start..end]
}

/// Registers the three JS workloads. Runtime creation and script compilation
/// happen here, outside the timed window, as in the rest of the suite.
pub(super) fn register_js_hot(suite: &mut Suite, scale: f64) {
    let items = generators::scale(ITEMS, scale);
    let reps = generators::scale(REPS, scale);
    let html = fixture_html(items);
    let bindings_expected = (items - 1).to_string();

    let engine_setup = engine_setup_script(items);
    let engine_script = engine_script(items, reps);
    let bindings_script = bindings_script(items, reps);
    let corpus_script = inline_script(CORPUS_HTML).to_owned();

    let mut engine = megi_js::JsRuntime::new().expect("JS engine runtime");
    let mut bindings = megi_js::JsRuntime::with_document(
        megi_html::parse_document(&html, false),
        "https://bench.example/js-hot".to_owned(),
    )
    .expect("JS runtime with DOM document");
    let mut corpus = megi_js::JsRuntime::with_document(
        megi_html::parse_document(
            include_str!("../../../../tests/fixtures/js_corpus/wikipedia_null.html"),
            false,
        ),
        "https://bench.example/js-corpus".to_owned(),
    )
    .expect("JS runtime with corpus document");

    // Fail at setup, not silently inside a timed sample.
    engine
        .eval(&engine_setup, "js_hot_setup")
        .expect("engine setup");
    assert_eq!(
        engine
            .eval(&engine_script, "js_hot_probe")
            .expect("engine probe"),
        "x"
    );
    assert_eq!(
        bindings
            .eval(&bindings_script, "js_hot_probe")
            .expect("bindings probe"),
        bindings_expected
    );
    corpus
        .eval(&corpus_script, "js_hot_corpus_probe")
        .expect("corpus probe");

    let calls = (items * reps) as f64;
    suite.add(
        BenchEntry::new(
            "js/hot-engine",
            "js",
            format!("{items} shims × {reps} getAttribute calls, no Rust bridge"),
            move || {
                black_box(
                    engine
                        .eval(&engine_script, "js_hot_engine")
                        .expect("engine"),
                );
            },
        )
        .with_units(calls, "calls"),
    );
    suite.add(
        BenchEntry::new(
            "js/hot-bindings",
            "js",
            format!("{items} DOM nodes × {reps} getAttribute calls"),
            move || {
                black_box(
                    bindings
                        .eval(&bindings_script, "js_hot_bindings")
                        .expect("bindings"),
                );
            },
        )
        .with_units(calls, "calls"),
    );
    suite.add(
        BenchEntry::new(
            "js/hot-corpus-wikipedia-null",
            "js",
            "real inline script from tests/fixtures/js_corpus/wikipedia_null.html",
            move || {
                black_box(
                    corpus
                        .eval(&corpus_script, "js_hot_corpus")
                        .expect("corpus"),
                );
            },
        )
        .with_units(1.0, "eval"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_variant_has_no_dom_bridge() {
        let script = engine_script(ITEMS, REPS);
        assert!(!script.contains("document"), "{script}");
        assert!(script.contains("getAttribute"));
    }

    #[test]
    fn engine_script_has_no_setup_work() {
        let script = engine_script(ITEMS, REPS);
        assert!(!script.contains("new Array"), "{script}");
        assert!(engine_setup_script(ITEMS).contains("new Array"));
    }

    #[test]
    fn binding_variant_uses_real_dom() {
        let script = bindings_script(ITEMS, REPS);
        assert!(script.contains("document.querySelectorAll"));
        assert!(script.contains("getAttribute"));
    }

    #[test]
    fn paired_variants_do_same_call_count() {
        let engine = engine_script(ITEMS, REPS);
        let bindings = bindings_script(ITEMS, REPS);
        assert_eq!(
            engine.matches("getAttribute('data-index')").count(),
            bindings.matches("getAttribute('data-index')").count()
        );
    }

    #[test]
    fn corpus_fixture_script_is_extracted() {
        let script = inline_script(CORPUS_HTML);
        assert!(script.contains("querySelectorAll") || script.contains("document.cookie"));
        assert!(!script.contains("<script>"));
    }

    #[test]
    fn probe_values_are_deterministic() {
        let html = fixture_html(8);
        let document = megi_html::parse_document(&html, false);
        let mut runtime =
            megi_js::JsRuntime::with_document(document, "https://bench.example/js-hot".to_owned())
                .expect("runtime");
        let script = bindings_script(8, 2);
        assert_eq!(runtime.eval(&script, "probe").expect("eval"), "7");
    }
}
