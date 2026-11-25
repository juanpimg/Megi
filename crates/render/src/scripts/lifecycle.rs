//! Lifecycle/host event dispatch and finalization after the script evaluation loop.

use megi_core::terminal_text::sanitize_terminal_text;

use super::VIRTUAL_EPOCH_MS;
use super::event_loop::{
    EventLoopState, drain_microtasks, resolve_host_dom_target, run_event_loop, truncate_utf8,
};
use super::providers::{HistoryActionQueue, LocationActionQueue};
use super::types::{ConsoleMessage, ScriptExecution, ScriptingHostContext, ScriptingLimits};

// Verbatim move of the finalization stage out of `execute_scripts`: keeping the original
// argument list avoids reshaping the moved code and preserves its call ordering exactly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_lifecycle_events(
    runtime: &mut megi_js::JsRuntime,
    limits: &ScriptingLimits,
    host: &ScriptingHostContext,
    module_tasks: usize,
    module_virtual_ms: u64,
    execution: &mut ScriptExecution,
    location_actions: &LocationActionQueue,
    history_actions: &HistoryActionQueue,
    warnings: &mut Vec<String>,
) {
    let event_loop_start = std::time::Instant::now();
    // Lifecycle events and timers run with a fresh budget: the per-script loop budget must
    // not be exhausted by the last script when its `DOMContentLoaded` listener fires.
    runtime.set_budget(megi_js::Budget {
        loop_iterations: limits.budget.loop_iterations,
        recursion_limit: limits.budget.recursion_limit,
        stack_size_limit: limits.budget.stack_size_limit,
    });

    let mut loop_state = EventLoopState {
        // Timer callbacks already spent driving top-level `await` count against the budget.
        tasks: module_tasks,
        virtual_time_ms: VIRTUAL_EPOCH_MS.saturating_add(module_virtual_ms),
        ..EventLoopState::default()
    };
    drain_microtasks(runtime, warnings);

    runtime.set_ready_state("interactive");
    // 1. `DOMContentLoaded` (after scripts, before timers), dispatched at the real
    // `document` target with bubbling: `document.addEventListener('DOMContentLoaded', ...)`
    // listeners fire, then capture/bubble visitors and (with `bubbles: true`) the single
    // bubble leg to `window`, exactly like a browser.
    match runtime.dispatch_event(megi_js::JsEventTarget::Document, "DOMContentLoaded", true) {
        Ok(_) => execution.events_dispatched += 1,
        Err(error) => warnings.push(format!("dispatching DOMContentLoaded failed: {error}")),
    }
    drain_microtasks(runtime, warnings);

    // 2. Timers (delays included) on the virtual clock, bounded by max_tasks/max_virtual_time.
    run_event_loop(runtime, limits, &mut loop_state, warnings);

    runtime.set_ready_state("complete");
    // 3. `load` on `window`, then timers scheduled by `load` listeners (same budget).
    match runtime.dispatch_event(megi_js::JsEventTarget::Window, "load", true) {
        Ok(_) => execution.events_dispatched += 1,
        Err(error) => warnings.push(format!("dispatching load failed: {error}")),
    }
    drain_microtasks(runtime, warnings);
    run_event_loop(runtime, limits, &mut loop_state, warnings);

    // 4. Host-dispatched events (e.g. `hashchange`/`popstate` after a shell navigation).
    // Listeners run on `window`, may mutate the DOM and schedule timers; microtasks and the
    // timer queue are pumped again so the final re-render sees their effects.
    const MAX_HOST_EVENTS: usize = 16;
    const MAX_HOST_EVENT_NAME: usize = 256;
    for event in host.host_events.iter().take(MAX_HOST_EVENTS) {
        let event = if event.len() > MAX_HOST_EVENT_NAME {
            // Truncate on a UTF-8 boundary: byte 256 may fall inside a multi-byte code
            // point in a host-supplied event name (#1216).
            let end = event
                .char_indices()
                .take_while(|(index, _)| *index < MAX_HOST_EVENT_NAME)
                .map(|(index, ch)| index + ch.len_utf8())
                .last()
                .unwrap_or(0);
            &event[..end]
        } else {
            event.as_str()
        };
        match runtime.dispatch_event(megi_js::JsEventTarget::Window, event, false) {
            Ok(_) => execution.events_dispatched += 1,
            Err(error) => warnings.push(format!("dispatching host event {event} failed: {error}")),
        }
        drain_microtasks(runtime, warnings);
        run_event_loop(runtime, limits, &mut loop_state, warnings);
    }
    for dom_event in host.host_dom_events.iter().take(MAX_HOST_EVENTS) {
        let resolved_target = {
            let doc = runtime.document();
            resolve_host_dom_target(&doc, &dom_event.target)
        };
        if dom_event.event_type == "focus" {
            if let megi_js::JsEventTarget::Node(node_id) = resolved_target {
                runtime.set_active_element(Some(node_id));
            }
        } else if dom_event.event_type == "blur" {
            runtime.set_active_element(None);
        }
        let kind = match &dom_event.kind {
            crate::HostUiEventKind::Simple => megi_js::JsUiEventKind::Simple,
            crate::HostUiEventKind::Focus => megi_js::JsUiEventKind::Focus,
            crate::HostUiEventKind::Composition { data } => {
                megi_js::JsUiEventKind::Composition { data: data.clone() }
            }
            crate::HostUiEventKind::Input { data, input_type } => megi_js::JsUiEventKind::Input {
                data: data.clone(),
                input_type: input_type.clone(),
            },
            crate::HostUiEventKind::Keyboard {
                key,
                code,
                ctrl_key,
                shift_key,
                alt_key,
                meta_key,
                repeat,
            } => megi_js::JsUiEventKind::Keyboard {
                key: key.clone(),
                code: code.clone(),
                ctrl_key: *ctrl_key,
                shift_key: *shift_key,
                alt_key: *alt_key,
                meta_key: *meta_key,
                repeat: *repeat,
            },
        };
        let event_name = truncate_utf8(&dom_event.event_type, MAX_HOST_EVENT_NAME);
        match runtime.dispatch_ui_event(
            resolved_target,
            event_name,
            dom_event.bubbles,
            dom_event.cancelable,
            &kind,
        ) {
            Ok(_) => execution.events_dispatched += 1,
            Err(error) => warnings.push(format!(
                "dispatching host UI event {event_name} failed: {error}"
            )),
        }
        drain_microtasks(runtime, warnings);
        run_event_loop(runtime, limits, &mut loop_state, warnings);
    }
    // 5. Deterministic cooperative workers and cross-frame postMessage delivery (#1544):
    // drain main<->worker and parent<->child iframe queues, then pump timers/microtasks.
    if host.enable_workers {
        // Worker deliveries get only the *remaining* task budget and are deliberately not
        // added to `loop_state.tasks`: that counter is the documented "timer callbacks"
        // stat, and counting deliveries there starves timers and inflates it (#1221).
        let worker_budget = limits.max_tasks.saturating_sub(loop_state.tasks);
        match runtime.run_worker_tasks(worker_budget) {
            Ok(_) => {}
            Err(error) => warnings.push(format!("worker tasks failed: {error}")),
        }
        drain_microtasks(runtime, warnings);
        run_event_loop(runtime, limits, &mut loop_state, warnings);
    }
    runtime.init_connected_frames();
    let frame_budget = limits.max_tasks.saturating_sub(loop_state.tasks).min(256);
    if let Err(error) = runtime.run_frame_tasks(frame_budget) {
        warnings.push(format!("frame tasks failed: {error}"));
    }
    drain_microtasks(runtime, warnings);
    run_event_loop(runtime, limits, &mut loop_state, warnings);

    // 6. CSS animation/transition timeline events (#941, #942). Checked lazily so pages
    // with no timeline listeners incur no style-resolution or event-object cost.
    const TIMELINE_EVENT_NAMES: [&str; 6] = [
        "animationstart",
        "animationiteration",
        "animationend",
        "transitionrun",
        "transitionstart",
        "transitionend",
    ];
    const MAX_TIMELINE_EVENTS: usize = 256;
    if runtime.has_any_event_listener(&TIMELINE_EVENT_NAMES)
        && let Some(provider) = &host.timeline_events_provider
    {
        let virtual_elapsed_ms = loop_state.virtual_time_ms.saturating_sub(VIRTUAL_EPOCH_MS) as f64;
        let effective_time_ms = host.animation_time_ms.max(virtual_elapsed_ms) as f32;
        let timeline_events = {
            let doc_snapshot = runtime.document();
            provider(&doc_snapshot, effective_time_ms)
        };
        for event in timeline_events.into_iter().take(MAX_TIMELINE_EVENTS) {
            let kind = match event.kind {
                super::TimelineEventKind::Animation {
                    animation_name,
                    elapsed_time_secs,
                } => megi_js::JsUiEventKind::Animation {
                    animation_name,
                    elapsed_time: elapsed_time_secs,
                    pseudo_element: String::new(),
                },
                super::TimelineEventKind::Transition {
                    property_name,
                    elapsed_time_secs,
                } => megi_js::JsUiEventKind::Transition {
                    property_name,
                    elapsed_time: elapsed_time_secs,
                    pseudo_element: String::new(),
                },
            };
            if let Err(error) = runtime.dispatch_ui_event(
                megi_js::JsEventTarget::Node(event.target),
                event.event_type,
                true,
                false,
                &kind,
            ) {
                warnings.push(format!(
                    "dispatching timeline event {} failed: {error}",
                    event.event_type
                ));
            } else {
                execution.events_dispatched += 1;
            }
        }
        drain_microtasks(runtime, warnings);
        run_event_loop(runtime, limits, &mut loop_state, warnings);
    }

    // Final microtask checkpoint: MutationObserver records queued by the last timer/host
    // callback callbacks are delivered **before** the document is handed back, so their
    // own DOM mutations are included in the re-render.
    drain_microtasks(runtime, warnings);

    if let Some(error) = runtime.last_event_error() {
        warnings.push(format!("uncaught script error: {error}"));
    }
    execution.event_loop = event_loop_start.elapsed();
    execution.tasks_ran = loop_state.tasks;
    execution.loop_iterations = loop_state.iterations;
    execution.virtual_time_ms = loop_state.virtual_time_ms.saturating_sub(VIRTUAL_EPOCH_MS);
    execution.limited |= loop_state.limited;
    execution.timers_pending = runtime.pending_timers();

    for entry in runtime.take_console() {
        // #2309: console text is page-controlled and is mirrored into warnings
        // printed to the operator's stderr; strip terminal controls at the source.
        let message = ConsoleMessage {
            level: sanitize_terminal_text(&entry.level),
            message: sanitize_terminal_text(&entry.message),
        };
        if matches!(message.level.as_str(), "warn" | "error") {
            warnings.push(format!("console.{}: {}", message.level, message.message));
        }
        execution.console.push(message);
    }
    execution.document = std::mem::take(&mut *runtime.document_mut());
    execution.canvas_surfaces = runtime.canvas_surfaces();
    execution.frame_documents = runtime.snapshot_frame_documents();
    execution.location_actions = std::mem::take(
        &mut *location_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    execution.history_actions = std::mem::take(
        &mut *history_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
}
