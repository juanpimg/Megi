//! WP-A tests: EventTarget/Event/CustomEvent and the injected-clock timer scheduler.

#![cfg(feature = "js")]

use std::time::Duration;

use js::{Budget, JsRuntime};

fn runtime() -> JsRuntime {
    let mut runtime = JsRuntime::new().expect("runtime");
    runtime.set_budget(Budget::SMALL);
    runtime
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[test]
fn click_bubbles_from_child_to_parent_and_window() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var log = [];
             var parent = document.createElement('div');
             var child = document.createElement('span');
             document.body.appendChild(parent);
             parent.appendChild(child);
             parent.addEventListener('click', function (event) { log.push('parent:' + event.type); });
             child.addEventListener('click', function (event) { log.push('child:' + event.type); });
             window.addEventListener('click', function () { log.push('window'); });
             child.dispatchEvent(new Event('click', { bubbles: true }));",
            "events.js",
        )
        .expect("event dispatch");
    assert_eq!(
        runtime.eval("log.join(',')", "events.js").expect("log"),
        "child:click,parent:click,window"
    );
}

#[test]
fn capture_listeners_run_before_bubble_listeners() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var order = [];
             var parent = document.createElement('div');
             var child = document.createElement('span');
             document.body.appendChild(parent);
             parent.appendChild(child);
             parent.addEventListener('ping', function () { order.push('capture'); }, true);
             child.addEventListener('ping', function () { order.push('target'); });
             child.dispatchEvent(new Event('ping', { bubbles: true }));",
            "capture.js",
        )
        .expect("capture dispatch");
    assert_eq!(
        runtime
            .eval("order.join(',')", "capture.js")
            .expect("order"),
        "capture,target"
    );
}

#[test]
fn prevent_default_marks_the_event() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var button = document.createElement('button');
             document.body.appendChild(button);
             button.addEventListener('submit', function (event) { event.preventDefault(); });
             var event = new Event('submit', { bubbles: true, cancelable: true });
             var result = button.dispatchEvent(event);",
            "prevent.js",
        )
        .expect("preventDefault dispatch");
    assert_eq!(
        runtime.eval("result", "prevent.js").expect("result"),
        "false",
        "dispatchEvent returns false when prevented"
    );
    assert_eq!(
        runtime
            .eval("event.defaultPrevented", "prevent.js")
            .expect("defaultPrevented"),
        "true"
    );
}

#[test]
fn stop_propagation_prevents_ancestor_listeners() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var seen = [];
             var parent = document.createElement('div');
             var child = document.createElement('span');
             document.body.appendChild(parent);
             parent.appendChild(child);
             child.addEventListener('stop', function (event) { seen.push('child'); event.stopPropagation(); });
             parent.addEventListener('stop', function () { seen.push('parent'); });
             child.dispatchEvent(new Event('stop', { bubbles: true }));",
            "stop.js",
        )
        .expect("stopPropagation");
    assert_eq!(
        runtime.eval("seen.join(',')", "stop.js").expect("seen"),
        "child"
    );
}

#[test]
fn remove_event_listener_stops_delivery() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var hits = 0;
             var element = document.createElement('div');
             document.body.appendChild(element);
             function onEvent() { hits += 1; }
             element.addEventListener('tick', onEvent);
             element.dispatchEvent(new Event('tick'));
             element.removeEventListener('tick', onEvent);
             element.dispatchEvent(new Event('tick'));",
            "remove.js",
        )
        .expect("removeEventListener");
    assert_eq!(runtime.eval("hits", "remove.js").expect("hits"), "1");
}

#[test]
fn listener_can_mutate_the_dom() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var target = document.createElement('div');
             target.id = 'target';
             document.body.appendChild(target);
             target.addEventListener('build', function () {
                 var child = document.createElement('b');
                 child.textContent = 'from listener';
                 target.appendChild(child);
             });
             target.dispatchEvent(new Event('build'));",
            "mutate.js",
        )
        .expect("listener mutation");
    let document = runtime.document();
    let target = document.get_element_by_id("target").expect("target");
    assert_eq!(document.text_content(target), "from listener");
}

#[test]
fn custom_event_carries_detail() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var got = null;
             var element = document.createElement('div');
             document.body.appendChild(element);
             element.addEventListener('custom', function (event) { got = event.detail; });
             element.dispatchEvent(new CustomEvent('custom', { detail: 42 }));",
            "custom.js",
        )
        .expect("CustomEvent");
    assert_eq!(runtime.eval("got", "custom.js").expect("detail"), "42");
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

#[test]
fn zero_timeout_runs_when_pumped() {
    let mut runtime = runtime();
    runtime
        .eval(
            "globalThis.ran = 0; setTimeout(function () { globalThis.ran += 1; }, 0);",
            "t0.js",
        )
        .expect("schedule");
    assert_eq!(runtime.pending_timers(), 1);
    let ran = runtime.run_tasks(10).expect("run tasks");
    assert!(ran >= 1);
    assert_eq!(runtime.eval("ran", "t0.js").expect("ran"), "1");
    assert_eq!(runtime.pending_timers(), 0);
}

#[test]
fn delayed_timeout_waits_for_the_injected_clock() {
    let mut runtime = runtime();
    let base = runtime
        .advance(Duration::ZERO)
        .expect("pin the virtual clock");
    runtime
        .eval(
            "var fired = 0; setTimeout(function () { fired += 1; }, 10);",
            "t10.js",
        )
        .expect("schedule");
    assert_eq!(runtime.run_until(base + 5).expect("clock to +5ms"), 0);
    assert_eq!(runtime.eval("fired", "t10.js").expect("fired"), "0");
    runtime
        .run_until(base + 10)
        .expect("virtual clock to +10ms");
    assert_eq!(runtime.eval("fired", "t10.js").expect("fired"), "1");
}

#[test]
fn timer_arguments_are_forwarded() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var received = null;
             setTimeout(function (first, second) { received = first + ':' + second; }, 0, 'a', 'b');",
            "args.js",
        )
        .expect("schedule");
    runtime.run_tasks(10).expect("pump");
    assert_eq!(runtime.eval("received", "args.js").expect("args"), "a:b");
}

#[test]
fn interval_runs_multiple_times() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var ticks = 0;
             var id = setInterval(function () { ticks += 1; }, 10);",
            "interval.js",
        )
        .expect("schedule");
    runtime
        .advance(Duration::from_millis(25))
        .expect("advance 25ms");
    assert_eq!(runtime.eval("ticks", "interval.js").expect("ticks"), "2");
    runtime
        .advance(Duration::from_millis(10))
        .expect("advance 10ms");
    assert_eq!(runtime.eval("ticks", "interval.js").expect("ticks"), "3");
    runtime
        .eval("clearInterval(id)", "interval.js")
        .expect("clear");
    runtime
        .advance(Duration::from_millis(100))
        .expect("advance");
    assert_eq!(runtime.eval("ticks", "interval.js").expect("ticks"), "3");
}

#[test]
fn clear_timeout_prevents_the_callback() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var fired = 0;
             var id = setTimeout(function () { fired += 1; }, 5);
             clearTimeout(id);",
            "clear.js",
        )
        .expect("schedule");
    assert_eq!(runtime.pending_timers(), 0);
    runtime.run_until(100).expect("run");
    assert_eq!(runtime.eval("fired", "clear.js").expect("fired"), "0");
}

#[test]
fn timers_run_in_due_then_insertion_order() {
    let mut runtime = runtime();
    let _ = runtime.advance(Duration::ZERO).expect("pin clock");
    runtime
        .eval(
            "var order = [];
             setTimeout(function () { order.push('late'); }, 10);
             setTimeout(function () { order.push('early-a'); }, 5);
             setTimeout(function () { order.push('early-b'); }, 5);",
            "order.js",
        )
        .expect("schedule");
    runtime.run_until(10).expect("run until 10ms");
    assert_eq!(
        runtime.eval("order.join(',')", "order.js").expect("order"),
        "early-a,early-b,late"
    );
}

#[test]
fn timer_can_schedule_another_timer() {
    let mut runtime = runtime();
    runtime
        .eval(
            "var log = [];
             setTimeout(function () {
                 log.push('outer');
                 setTimeout(function () { log.push('inner'); }, 0);
             }, 0);",
            "nested.js",
        )
        .expect("schedule");
    let ran = runtime.run_tasks(10).expect("pump");
    assert!(ran >= 2, "nested timer must also run, ran={ran}");
    assert_eq!(
        runtime.eval("log.join(',')", "nested.js").expect("log"),
        "outer,inner"
    );
}
