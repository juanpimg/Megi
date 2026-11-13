#![no_main]
//! Fuzz target: JavaScript evaluation + DOM/event/timer/clock host bridge with a strict budget.
//!
//! Each iteration builds a fresh realm against a small deterministic DOM fixture selected from
//! the input, derives bounded budget/clock parameters from input bytes, evaluates one or more
//! script slices, dispatches a short event sequence, and advances the virtual clock/timer queue
//! in bounded steps. This explores budget/clock/DOM variation and limited realm reuse without
//! unbounded work.
//!
//! Host providers (`fetch`, Web Storage, layout, viewport, media, computed style, history,
//! location, worker loader) are installed as **hostile stubs** derived from the input (#704):
//! reentrant/typed-error rejections, NaN/±∞ geometry, conflicting response headers and
//! oversized bodies are all reachable, so the script↔provider↔DOM glue is exercised instead
//! of always seeing the "no provider installed" default path.
//!
//! The target looks for panics/aborts/UB/timeouts, not for semantic mismatches; no network,
//! filesystem or wall-clock sleep is reachable from the realm.

use libfuzzer_sys::fuzz_target;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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

/// Probes every injectable host provider from one deterministic script, so provider glue is
/// reached even when the fuzzer input contains no relevant code.
const PROVIDER_PROBE: &str = "\
try{fetch('/x').then(function(r){return r.text()}).then(function(t){window.__p1=t.length}).catch(function(e){window.__p1=-1})}catch(e){}\n\
try{localStorage.setItem('k','v');window.__p2=localStorage.getItem('k')}catch(e){}\n\
try{window.__p3=matchMedia('(min-width:1px)').matches}catch(e){}\n\
try{window.__p4=document.body.getBoundingClientRect().width}catch(e){}\n\
try{window.__p5=window.innerWidth+window.devicePixelRatio}catch(e){}\n\
try{var w=new Worker('/w.js');w.onmessage=function(){window.__p6=1};w.postMessage(1)}catch(e){}\n\
try{window.__p7=getComputedStyle(document.body).color}catch(e){}\n\
try{history.pushState({},'','/p')}catch(e){}\n\
try{location.hash='#h'}catch(e){}\n";

/// Installs bounded, offline provider stubs whose behaviour is derived from `seed`.
///
/// Every provider caps its own call count so a script that loops on it still terminates; the
/// returned values include the shapes that historically break hosts (NaN/±∞ geometry, a
/// status of `0`, conflicting `content-length` headers, bodies that exceed the advertised
/// length, storage keys with NULs).
fn install_hostile_providers(runtime: &mut js::JsRuntime, seed: &[u8]) {
    let seed: Arc<Vec<u8>> = Arc::new(seed.iter().copied().take(64).collect());

    // fetch(): typed error, hostile headers/status, or a body larger than the caller expects.
    let fetch_seed = Arc::clone(&seed);
    let fetch_calls = Arc::new(AtomicUsize::new(0));
    runtime.set_fetch_provider(Box::new(move |request| {
        let call = fetch_calls.fetch_add(1, Ordering::Relaxed);
        if call >= 4 {
            return Err(js::JsError::Budget(
                "fuzz fetch provider call cap".to_string(),
            ));
        }
        let b = fetch_seed
            .get(call % fetch_seed.len().max(1))
            .copied()
            .unwrap_or(0);
        match b % 4 {
            0 => Err(js::JsError::Runtime(
                "hostile provider rejection".to_string(),
            )),
            1 => Ok(js::host::FetchResult {
                status: 0,
                status_text: String::new(),
                url: request.url,
                headers: vec![("X-Fuzz".to_string(), "\r\n\t\"".to_string())],
                body: vec![0xFF; usize::from(b % 32)],
            }),
            2 => Ok(js::host::FetchResult {
                status: 599,
                status_text: "hostile".to_string(),
                url: request.url,
                // Conflicting lengths: hosts that trust the header must not panic.
                headers: vec![
                    ("content-length".to_string(), "1".to_string()),
                    ("content-length".to_string(), "999999".to_string()),
                ],
                body: vec![b; usize::from(b % 128)],
            }),
            _ => Ok(js::host::FetchResult {
                status: 200,
                status_text: "OK".to_string(),
                url: request.url,
                headers: vec![("content-type".to_string(), "text/plain".to_string())],
                body: vec![b; usize::from(b)],
            }),
        }
    }));

    // Storage: hostile persisted entries (NULs, empty keys, long values) + bounded saves.
    let storage_seed = Arc::clone(&seed);
    let save_calls = Arc::new(AtomicUsize::new(0));
    let saves = Arc::clone(&save_calls);
    runtime.set_storage_provider(js::host::StorageProvider {
        load: Box::new(move |_kind| {
            vec![
                (String::new(), "empty-key".to_string()),
                ("k\u{0}ey".to_string(), "v".to_string()),
                (
                    "k".to_string(),
                    "x".repeat(
                        usize::from(
                            storage_seed
                                .get(3 % storage_seed.len().max(1))
                                .copied()
                                .unwrap_or(0),
                        ) * 8,
                    ),
                ),
            ]
        }),
        save: Box::new(move |_kind, _entries| {
            saves.fetch_add(1, Ordering::Relaxed);
        }),
    });

    // Layout: NaN/±∞/negative boxes, or no box at all.
    let layout_seed = Arc::clone(&seed);
    runtime.set_layout_provider(Box::new(move |_node| {
        match layout_seed.first().copied().unwrap_or(0) % 3 {
            0 => None,
            1 => Some(js::host::LayoutRect {
                border_box: (f32::NAN, f32::NEG_INFINITY, 1.0e30, -1.0),
                content_box: (0.0, 0.0, -0.0, f32::INFINITY),
            }),
            _ => Some(js::host::LayoutRect {
                border_box: (1.0, 2.0, 3.0, 4.0),
                content_box: (1.0, 2.0, 3.0, 4.0),
            }),
        }
    }));

    // Viewport: zero / u32::MAX sizes and non-finite offsets/DPR.
    let viewport_seed = Arc::clone(&seed);
    runtime.set_viewport_provider(Box::new(move || {
        let b = |i: usize| u32::from(viewport_seed.get(i).copied().unwrap_or(0));
        js::ViewportInfo {
            width: b(0).wrapping_mul(97),
            height: b(1).wrapping_mul(89),
            device_pixel_ratio: if b(2) % 2 == 0 { f64::NAN } else { 1.0e30 },
            scroll_x: f64::from(b(3)) * 1.0e12,
            scroll_y: -(f64::from(b(4)) * 1.0e12),
        }
    }));

    let media_seed = Arc::clone(&seed);
    runtime.set_media_provider(Box::new(move |_query| {
        media_seed
            .get(5 % media_seed.len().max(1))
            .copied()
            .unwrap_or(0)
            % 2
            == 0
    }));

    // Computed style: hostile property names/values, including an empty list.
    let style_seed = Arc::clone(&seed);
    runtime.set_computed_style_provider(Box::new(move |_doc, _node| {
        if style_seed.first().copied().unwrap_or(0) % 3 == 0 {
            return Vec::new();
        }
        vec![
            ("color".to_string(), "not-a-color".to_string()),
            (String::new(), String::new()),
            ("x".repeat(64), "\u{0}{}".to_string()),
        ]
    }));

    // History/location: record-and-ignore (no navigation happens in the harness).
    let history_calls = Arc::new(AtomicUsize::new(0));
    let hist = Arc::clone(&history_calls);
    runtime.set_history_provider(Box::new(move |_action| {
        hist.fetch_add(1, Ordering::Relaxed);
    }));
    let location_calls = Arc::new(AtomicUsize::new(0));
    let loc = Arc::clone(&location_calls);
    runtime.set_location_provider(Box::new(move |_action| {
        loc.fetch_add(1, Ordering::Relaxed);
    }));

    // Worker loader: bounded calls, typed error after the cap, source from the seed.
    let worker_seed = Arc::clone(&seed);
    let worker_calls = Arc::new(AtomicUsize::new(0));
    runtime.set_worker_loader(Box::new(move |_url| {
        let call = worker_calls.fetch_add(1, Ordering::Relaxed);
        if call >= 2 {
            return Err(js::JsError::Budget(
                "fuzz worker loader call cap".to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&worker_seed);
        Ok(format!(
            "self.onmessage=function(e){{self.postMessage(1)}};/*{text}*/"
        ))
    }));
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

    install_hostile_providers(&mut runtime, &data[5.min(data.len())..]);
    // Deterministic probe: reaches every provider even when the input has no relevant code.
    let _ = runtime.eval(PROVIDER_PROBE, "provider_probe.js");
    let _ = runtime.run_microtasks();
    let _ = runtime.run_tasks(16);

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
