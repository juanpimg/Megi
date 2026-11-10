#![no_main]
//! Fuzz target: JavaScript evaluation against a small DOM with a strict budget.
//!
//! Each input becomes a script evaluated in a fresh realm whose document is a fixed,
//! deterministic fixture (elements with ids/classes, an input, a form and a template). The
//! fuzzer therefore exercises the whole host bridge: DOM queries (`getElementById`,
//! `querySelector`), mutations (`textContent`, `innerHTML`, attributes, `style`,
//! `classList`), event listeners/timers (`addEventListener`, `setTimeout`,
//! `setInterval`), plus the parser/VM itself.
//!
//! Budgets are deliberately tiny ([`js::Budget`] 50k loop iterations, small recursion and
//! VM stack) and the timer pump is bounded, so malicious infinite loops terminate with a
//! typed [`js::JsError::Budget`] instead of hanging. After evaluation the target also
//! dispatches a lifecycle/click event derived from the input and drains microtasks/timers,
//! covering the event dispatch path.
//!
//! The target looks for panics/aborts/UB/timeouts, not for semantic mismatches; no network,
//! filesystem or wall-clock sleep is reachable from the realm.

use libfuzzer_sys::fuzz_target;

/// The fixture document every input runs against (parsed once per iteration; cheap).
const FIXTURE_HTML: &str = "<!DOCTYPE html><html><head><title>fuzz</title></head><body>\
<div id=root class=container><p id=text class=fuzz>hello fuzz</p>\
<ul><li class=item>one<li class=item>two</ul>\
<form><input id=field value=seed><button id=btn>go</button></form>\
<template><span class=inner>tpl</span></template></div>\
<script>window.__fuzz_marker = 1;</script></body></html>";

fuzz_target!(|data: &[u8]| {
    let script = String::from_utf8_lossy(data);
    let document = html::parse_document(FIXTURE_HTML, false);
    let Ok(mut runtime) = js::JsRuntime::with_document(document, "about:blank".to_string())
    else {
        return;
    };
    runtime.set_budget(js::Budget {
        loop_iterations: 50_000,
        recursion_limit: 128,
        stack_size_limit: 4_096,
    });
    let _ = runtime.eval(&script, "fuzz.js");
    let _ = runtime.run_microtasks();
    let _ = runtime.run_tasks(64);

    // Lifecycle/click dispatch derived deterministically from the input.
    const EVENTS: [&str; 4] = ["DOMContentLoaded", "load", "click", "input"];
    let event = EVENTS[data.first().copied().unwrap_or(0) as usize % EVENTS.len()];
    let target = match data.get(1).copied().unwrap_or(0) % 3 {
        0 => js::JsEventTarget::Window,
        1 => js::JsEventTarget::Document,
        _ => js::JsEventTarget::Node(runtime.document().root()),
    };
    let _ = runtime.dispatch_event(target, event, true);
    let _ = runtime.run_microtasks();
    let _ = runtime.run_tasks(64);
    let _ = runtime.take_document();
});
