#![no_main]
//! Fuzz target: JavaScript evaluation + DOM/event/timer/clock host bridge with a strict budget.
//!
//! Each iteration builds a fresh realm against a small deterministic DOM fixture selected from
//! the input, derives bounded budget/clock parameters from input bytes, evaluates one or more
//! script slices, dispatches a short event sequence, and advances the virtual clock/timer queue
//! in bounded steps. This explores budget/clock/DOM variation and limited realm reuse without
//! unbounded work.
//!
//! The target looks for panics/aborts/UB/timeouts, not for semantic mismatches; no network,
//! filesystem or wall-clock sleep is reachable from the realm.

use libfuzzer_sys::fuzz_target;
use std::time::Duration;

/// Deterministic fixture documents selected from input bytes.
const FIXTURES: [&str; 4] = [
    "<!DOCTYPE html><html><head><title>fuzz</title></head><body>\
<div id=root class=container><p id=text class=fuzz>hello fuzz</p>\
<ul><li class=item>one<li class=item>two</ul>\
<form><input id=field value=seed><button id=btn>go</button></form>\
<template><span class=inner>tpl</span></template></div>\
<script>window.__fuzz_marker = 1;</script></body></html>",
    "<!DOCTYPE html><html><body><div id=app data-x='1'><span class=a>alpha</span>\
<span class=b>beta</span><input id=i type=text value='v'><button id=go>go</button></div></body></html>",
    "<!DOCTYPE html><html><body><table id=t><tr><td class=c>1</td><td class=c>2</td></tr></table>\
<ul id=list><li>one</li><li>two</li><li>three</li></ul><p id=p>text</p></body></html>",
    "<!DOCTYPE html><html><body><form id=f><input id=name name=n value=x>\
<input id=chk type=checkbox checked><select id=s><option>a</option><option selected>b</option></select>\
</form><div id=out></div></body></html>",
];

const EVENTS: [&str; 6] = [
    "DOMContentLoaded",
    "load",
    "click",
    "input",
    "change",
    "hashchange",
];

fn bounded_usize(byte: u8, min: usize, max: usize) -> usize {
    if max <= min {
        return min;
    }
    min + (byte as usize % (max - min + 1))
}

fn pick_target(runtime: &js::JsRuntime, selector: u8) -> js::JsEventTarget {
    match selector % 4 {
        0 => js::JsEventTarget::Window,
        1 => js::JsEventTarget::Document,
        2 => js::JsEventTarget::Node(runtime.document().root()),
        _ => {
            // Prefer a stable element when available; fall back to document root.
            let root = runtime.document().root();
            js::JsEventTarget::Node(root)
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let fixture = FIXTURES[data[0] as usize % FIXTURES.len()];
    let document = html::parse_document(fixture, false);
    let Ok(mut runtime) = js::JsRuntime::with_document(document, "about:blank".to_string()) else {
        return;
    };

    // Input-derived but capped budgets (bounded CPU/memory/time).
    let loop_iterations = bounded_usize(data.get(1).copied().unwrap_or(0), 1_000, 50_000) as u64;
    let recursion_limit = bounded_usize(data.get(2).copied().unwrap_or(0), 16, 128);
    let stack_size_limit = bounded_usize(data.get(3).copied().unwrap_or(0), 512, 4_096);
    let time_budget_ms = bounded_usize(data.get(4).copied().unwrap_or(0), 5, 120) as u64;
    runtime.set_budget(js::Budget {
        loop_iterations,
        recursion_limit,
        stack_size_limit,
    });

    // Split input into bounded script slices for limited realm reuse.
    let script_bytes = &data[5.min(data.len())..];
    let script = String::from_utf8_lossy(script_bytes);
    let mut slices: Vec<&str> = script
        .split('\0')
        .filter(|s| !s.is_empty())
        .take(3)
        .collect();
    if slices.is_empty() && !script.is_empty() {
        let mid = (script.len() / 2).min(2_048);
        if mid > 0 && mid < script.len() {
            slices.push(&script[..mid]);
            slices.push(&script[mid..script.len().min(mid + 2_048)]);
        } else {
            slices.push(&script[..script.len().min(4_096)]);
        }
    }

    let eval_rounds = slices.len().max(1).min(2);
    for (i, src) in slices.into_iter().take(eval_rounds).enumerate() {
        let name = if i == 0 { "fuzz.js" } else { "fuzz_reuse.js" };
        let _ = runtime.eval(src, name);
        let _ = runtime.run_microtasks();
        let _ = runtime.run_tasks(32);

        // Short deterministic event sequence derived from input.
        let seq_len = 1 + (data.get(6 + i).copied().unwrap_or(0) as usize % 3);
        for j in 0..seq_len {
            let b = data.get(8 + i * 4 + j).copied().unwrap_or((i + j) as u8);
            let event = EVENTS[b as usize % EVENTS.len()];
            let target = pick_target(&runtime, data.get(9 + i * 4 + j).copied().unwrap_or(b));
            let _ = runtime.dispatch_event(target, event, true);
            let _ = runtime.run_microtasks();
            let _ = runtime.run_tasks(16);
        }

        // Advance virtual clock in bounded steps to exercise timers/scheduler.
        let step_ms = bounded_usize(
            data.get(20 + i).copied().unwrap_or(10),
            0,
            time_budget_ms as usize,
        ) as u64;
        if step_ms > 0 {
            let _ = runtime.advance(Duration::from_millis(step_ms));
            let _ = runtime.run_microtasks();
            let _ = runtime.run_tasks(32);
        }
    }

    // Final drain and bounded clock advance.
    let _ = runtime.run_microtasks();
    let _ = runtime.run_tasks(64);
    let _ = runtime.advance(Duration::from_millis(time_budget_ms.min(50)));
    let _ = runtime.run_microtasks();
    let _ = runtime.run_tasks(32);
    let _ = runtime.take_document();
});
