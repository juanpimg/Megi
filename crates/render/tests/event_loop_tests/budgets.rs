//! Task/virtual-time budget enforcement for timers and module loops.

use super::common::*;

#[test]
fn task_budget_cuts_an_infinite_timer_loop() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 5,
            max_virtual_time_ms: 1000,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_loop.html", &options);
    assert!(output.stats.script_tasks <= 5);
    assert!(
        output.stats.script_timers_pending > 0,
        "loop should be pending"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("budget")),
        "missing budget warning: {:?}",
        output.stats.warnings
    );
    assert!(element_text(&output.document, "out").starts_with("ticks:"));
}

#[test]
fn virtual_time_budget_cuts_a_slow_timer_loop() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 10_000,
            max_virtual_time_ms: 25,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_loop.html", &options);
    assert!(output.stats.script_virtual_time_ms <= 25);
    assert!(output.stats.script_timers_pending > 0);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("virtual-time")),
        "{:?}",
        output.stats.warnings
    );
}

#[test]
fn a_timer_beyond_the_virtual_time_budget_stays_pending_without_mutating() {
    let options = RenderOptions {
        scripting_limits: ScriptingLimits {
            max_tasks: 100,
            max_virtual_time_ms: 5,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_fixture("js_event_timeout.html", &options);
    assert_eq!(element_text(&output.document, "out"), "before");
    assert_eq!(output.stats.script_tasks, 0);
    assert_eq!(output.stats.script_timers_pending, 1);
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("virtual-time"))
    );
}

#[test]
fn timer_task_budget_caps_a_single_batch() {
    // 100 zero-delay timers with `max_tasks = 10`: the loop must stop at the budget and
    // report it, instead of running a whole 100k batch inside one `run_until`.
    let html = "<html><body><script>window.__n = 0;\
                for (var i = 0; i < 100; i++) { setTimeout(function () { window.__n++; }, 0); }\
                </script><p id=\"out\">x</p></body></html>";
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        scripting_limits: ScriptingLimits {
            max_tasks: 10,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_url(&format!("data:text/html,{encoded}"), &options).expect("render");
    assert!(
        output.stats.script_tasks <= 10,
        "ran {} timer callbacks with max_tasks = 10",
        output.stats.script_tasks
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("timer task budget")),
        "{:?}",
        output.stats.warnings
    );
}

#[test]
fn top_level_await_respects_the_timer_budget() {
    // A module with top-level await drives timers while it waits. Those callbacks must
    // count against `max_tasks` and leave the rest pending; before the fix the module loop
    // drained up to 100,000 timers and reported none.
    let html = "<html><body>\
                <script>for (var i = 0; i < 200; i++) { setTimeout(function () {}, 0); }</script>\
                <script type=\"module\">await new Promise(function () {});</script>\
                </body></html>";
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        scripting_limits: ScriptingLimits {
            max_tasks: 10,
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_url(&format!("data:text/html,{encoded}"), &options).expect("render");
    assert!(
        output.stats.script_tasks <= 10,
        "ran {} timer callbacks with max_tasks = 10",
        output.stats.script_tasks
    );
    assert!(
        output.stats.script_timers_pending > 0,
        "the remaining timers must stay pending, not run inside the module loop"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|warning| warning.contains("timer task budget exhausted")),
        "hitting the timer budget must be reported: {:?}",
        output.stats.warnings
    );
}

#[test]
fn timer_callbacks_use_the_configured_loop_budget() {
    // The inline script gets a tiny grant from `total_loop_iterations`; the event loop must
    // then apply the (large) fresh budget to the timer callback instead of inheriting the
    // tiny limit left in the VM by the last eval (#1215).
    let html = "<html><body><script>\
        window.__done = 0;\
        setTimeout(function () {\
            var n = 0; for (var j = 0; j < 90000; j++) { n += 1; }\
            window.__done = 1;\
        }, 0);\
        </script></body></html>";
    let encoded: String = html
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    let options = RenderOptions {
        viewport: Viewport::new(400, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        scripting_limits: ScriptingLimits {
            max_tasks: 10,
            total_loop_iterations: 100,
            budget: ScriptBudget {
                loop_iterations: 100_000,
                ..ScriptBudget::default()
            },
            ..ScriptingLimits::default()
        },
        ..base_options()
    };
    let output = render_url(&format!("data:text/html,{encoded}"), &options).expect("render");
    assert_eq!(
        output.stats.script_tasks, 1,
        "the timer must run with the configured budget: {:?}",
        output.stats.warnings
    );
    assert!(
        !output.stats.script_limited,
        "a fresh 100k budget must not be cut by the inline script's 100-iteration grant: {:?}",
        output.stats.warnings
    );
}
