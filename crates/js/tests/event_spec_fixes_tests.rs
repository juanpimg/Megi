//! Regression tests for JS event/document specs (#1619, #1620, #1621).

#![cfg(feature = "js")]

use js::runtime::JsRuntime;

#[test]
fn custom_event_is_not_cancelable_by_default() {
    // #1619: CustomEventInit.cancelable defaults to false (DOM spec).
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var e = new CustomEvent('x'); String([e.cancelable, e.bubbles].join(','));",
            "t",
        )
        .expect("CustomEvent");
    assert_eq!(out, "false,false", "custom event defaults: {out}");
}

#[test]
fn writeln_appends_a_newline() {
    // #1620: document.writeln must append a newline, unlike write.
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "document.body.innerHTML=''; document.writeln('<b>X</b>'); String(document.body.innerHTML);",
            "t",
        )
        .expect("writeln");
    assert!(out.contains('\n'), "writeln must append a newline: {out:?}");
}

#[test]
fn event_without_type_throws_type_error() {
    // #1621: new Event() with no type must throw TypeError, not coerce to "undefined".
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var threw = false; try { new Event(); } catch (e) { threw = e instanceof TypeError; } String(threw);",
            "t",
        )
        .expect("Event()");
    assert_eq!(
        out, "true",
        "Event() without a type must throw TypeError: {out}"
    );
}
