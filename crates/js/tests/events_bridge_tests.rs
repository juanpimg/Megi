//! Focused regression tests for the `events.rs` → Context host-bridge migration.
//!
//! The structural guard pins the migration itself: every event binding must
//! borrow the host from the Boa `Context` (`host_cell` / `host_ref`) rather than
//! the legacy thread-local `with_host`. The behaviour guards exercise the risky
//! sites named in `.superpowers/sdd/2026-11-18-js-host-bridge/events-map.md`:
//! viewport/media providers, computed style, dispatch state and re-entrancy.

#![cfg(feature = "js")]

use std::sync::Arc;

use megi_js::JsRuntime;
use megi_js::host::ViewportInfo;

// The events bindings live in the `events` module; after the mechanical split the
// facade is thin and the bindings are spread across its submodules, so the guard
// must scan the whole module to keep checking the same property.
const EVENTS_SRC: &str = concat!(
    include_str!("../src/events.rs"),
    include_str!("../src/events/core.rs"),
    include_str!("../src/events/install.rs"),
    include_str!("../src/events/inline.rs"),
    include_str!("../src/events/event.rs"),
    include_str!("../src/events/listeners.rs"),
    include_str!("../src/events/dispatch.rs"),
    include_str!("../src/events/window.rs"),
    include_str!("../src/events/computed_style.rs"),
    include_str!("../src/events/timers.rs"),
);

#[test]
fn events_module_borrows_host_from_context_not_tls() {
    let cell_calls = EVENTS_SRC.matches("host_cell(").count();
    let ref_calls = EVENTS_SRC.matches("host_ref(").count();
    let tls_calls = EVENTS_SRC.matches("with_host(").count();

    assert!(
        cell_calls >= 20 && ref_calls >= 10,
        "events.rs must use the Context bridge (host_cell={cell_calls}, host_ref={ref_calls})"
    );
    // `now_ms()` is a compatibility shim with no `Context` parameter; it is the
    // only permitted legacy call until runtime.rs callers are threaded through.
    assert!(
        tls_calls <= 1,
        "events.rs must not use the legacy TLS bridge (found {tls_calls} with_host calls)"
    );
}

#[test]
fn viewport_and_media_providers_work_through_the_bridge() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_viewport_provider(Arc::new(|| ViewportInfo {
        width: 640,
        height: 480,
        device_pixel_ratio: 2.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }));
    runtime.set_media_provider(Arc::new(|query| query.contains("640px")));

    let out = runtime
        .eval(
            "[innerWidth, innerHeight, devicePixelRatio, \
             matchMedia('(max-width: 640px)').matches, \
             matchMedia('(min-width: 900px)').matches].join(',')",
            "events-bridge.js",
        )
        .expect("viewport/media eval");
    assert_eq!(out, "640,480,2,true,false");
}

#[test]
fn computed_style_provider_survives_the_bridge() {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_computed_style_provider(Arc::new(|_document, _node| {
        vec![("color".to_string(), "rgb(7, 8, 9)".to_string())]
    }));

    let out = runtime
        .eval(
            "var el = document.createElement('div'); \
             document.body.appendChild(el); \
             getComputedStyle(el).getPropertyValue('color')",
            "computed-bridge.js",
        )
        .expect("computed style eval");
    assert_eq!(out, "rgb(7, 8, 9)");
}

#[test]
fn dispatch_state_stays_stacked_through_the_bridge() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var log = []; \
             window.addEventListener('x', function(){ log.push('a'); }); \
             window.addEventListener('x', function(e){ log.push('b'); e.preventDefault(); }); \
             window.addEventListener('x', function(){ log.push('c'); }); \
             var notPrevented = window.dispatchEvent(new Event('x', {cancelable: true})); \
             log.join(',') + '|' + notPrevented",
            "dispatch-bridge.js",
        )
        .expect("dispatch eval");
    assert_eq!(out, "a,b,c|false");
}

#[test]
fn nested_dispatch_restores_outer_dispatch_state() {
    let mut runtime = JsRuntime::new().expect("runtime");
    let out = runtime
        .eval(
            "var log = []; \
             window.addEventListener('inner', function(e){ \
                 e.preventDefault(); log.push('inner'); \
             }); \
             window.addEventListener('outer', function(){ \
                 log.push('outer'); \
                 var nested = !window.dispatchEvent(new Event('inner', {cancelable: true})); \
                 log.push('nested:' + nested); \
             }); \
             var outer = !window.dispatchEvent(new Event('outer', {cancelable: true})); \
             log.push('outer:' + outer); \
             log.join(',')",
            "nested-bridge.js",
        )
        .expect("nested dispatch eval");
    assert_eq!(out, "outer,inner,nested:true,outer:false");
}
