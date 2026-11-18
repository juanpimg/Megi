//! Delayed, zero-delay and interval timers plus `load`-event timers.

use super::common::*;

#[test]
fn timer_with_delay_mutates_the_dom_and_the_png() {
    let enabled = render_fixture("js_event_timeout.html", &base_options());
    assert_eq!(element_text(&enabled.document, "out"), "after-timeout");
    assert!(enabled.stats.script_tasks >= 1, "timer did not run");
    assert!(
        enabled.stats.script_virtual_time_ms >= 10,
        "virtual clock did not advance: {}",
        enabled.stats.script_virtual_time_ms
    );
    assert!(enabled.stats.script_events >= 2, "lifecycle events missing");
    assert_eq!(enabled.stats.script_timers_pending, 0);
    assert!(
        enabled.stats.warnings.is_empty(),
        "{:?}",
        enabled.stats.warnings
    );

    let disabled = render_fixture("js_event_timeout.html", &disabled_options());
    assert_eq!(element_text(&disabled.document, "out"), "before");
    assert_eq!(disabled.stats.script_tasks, 0);
    assert_ne!(
        enabled.png, disabled.png,
        "timer mutation must be visible in the rendered PNG"
    );
}

#[test]
fn zero_delay_timer_runs_without_advancing_the_clock() {
    let output = render_fixture("js_event_timeout_zero.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "after-zero");
    assert!(output.stats.script_tasks >= 1);
    assert_eq!(output.stats.script_virtual_time_ms, 0);
    assert_eq!(output.stats.script_timers_pending, 0);
}

#[test]
fn interval_runs_three_ticks_then_clears_itself() {
    let output = render_fixture("js_event_interval.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "ticks:3");
    assert_eq!(
        output.stats.script_timers_pending, 0,
        "interval was not cleared"
    );
    assert!(output.stats.script_tasks >= 3);
    assert!(
        output.stats.script_virtual_time_ms >= 15,
        "expected >= 15 ms of virtual time, got {}",
        output.stats.script_virtual_time_ms
    );
}

#[test]
fn load_listener_and_its_timer_mutate_the_dom() {
    let output = render_fixture("js_event_load.html", &base_options());
    assert_eq!(element_text(&output.document, "out"), "load-timer-fired");
    assert!(output.stats.script_events >= 2);
    assert!(output.stats.script_tasks >= 1);
}
