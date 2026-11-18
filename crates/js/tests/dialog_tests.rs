//! #1546: HTMLDialogElement show/showModal/close and the `open` / `returnValue` API.

#![cfg(feature = "js")]

use megi_js::runtime::JsRuntime;

fn runtime() -> JsRuntime {
    JsRuntime::new().expect("runtime")
}

#[test]
fn dialog_show_and_close_toggle_open_and_fire_events() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var d = document.createElement('dialog'); document.body.appendChild(d); \
             var events = []; \
             d.addEventListener('toggle', function () { events.push('toggle'); }); \
             d.addEventListener('close', function () { events.push('close'); }); \
             var initial = d.open; \
             d.show(); var afterShow = d.open; \
             d.close('done'); var afterClose = d.open; \
             String([initial, afterShow, afterClose, d.returnValue, events.join(',')].join('|')) ;",
            "t",
        )
        .expect("dialog lifecycle");
    assert_eq!(
        out, "false|true|false|done|toggle,close",
        "dialog lifecycle: {out}"
    );
}

#[test]
fn dialog_show_modal_marks_modality() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var d = document.createElement('dialog'); document.body.appendChild(d); \
             d.showModal(); var modal = d.hasAttribute('data-megi-modal'); \
             d.close(); var cleared = d.hasAttribute('data-megi-modal'); \
             String(modal && !cleared && !d.open);",
            "t",
        )
        .expect("showModal");
    assert_eq!(out, "true");
}

#[test]
fn dialog_open_property_sets_attribute() {
    let mut runtime = runtime();
    let out = runtime
        .eval(
            "var d = document.createElement('dialog'); d.open = true; \
             var a = d.hasAttribute('open'); d.open = false; \
             String(a && !d.hasAttribute('open'));",
            "t",
        )
        .expect("open property");
    assert_eq!(out, "true");
}
