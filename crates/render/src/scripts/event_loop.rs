//! Deterministic virtual-time event loop and host-event target resolution.

use megi_core::{Document, NodeId};

use super::types::ScriptingLimits;
use super::{VIRTUAL_EPOCH_MS, VIRTUAL_TIME_STEP_MS};

/// Progress of one event-loop drain (shared by the initial timers and the `load` pass).
#[cfg(feature = "js")]
#[derive(Default)]
pub(crate) struct EventLoopState {
    pub(crate) tasks: usize,
    pub(crate) iterations: usize,
    pub(crate) virtual_time_ms: u64,
    pub(crate) limited: bool,
}

#[cfg(feature = "js")]
pub(crate) fn drain_microtasks(runtime: &mut megi_js::JsRuntime, warnings: &mut Vec<String>) {
    if let Err(error) = runtime.run_microtasks() {
        warnings.push(format!("microtasks failed: {error}"));
    }
}

#[cfg(feature = "js")]
pub(crate) fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let end = value
        .char_indices()
        .take_while(|(index, _)| *index < max_bytes)
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0);
    &value[..end]
}

#[cfg(feature = "js")]
fn find_element_with_attr(
    document: &Document,
    attr: &str,
    expected: Option<&str>,
) -> Option<NodeId> {
    let mut stack = vec![document.root()];
    while let Some(id) = stack.pop() {
        if let Some(node) = document.try_node(id) {
            if let Some(element) = node.as_element() {
                match expected {
                    Some(target_val) => {
                        if element.get_attribute(attr) == Some(target_val) {
                            return Some(id);
                        }
                    }
                    None => {
                        if element.has_attribute(attr) {
                            return Some(id);
                        }
                    }
                }
            }
            let mut children = Vec::new();
            let mut child = node.first_child;
            while let Some(child_id) = child {
                children.push(child_id);
                child = document.try_node(child_id).and_then(|n| n.next_sibling);
            }
            for child_id in children.into_iter().rev() {
                stack.push(child_id);
            }
        }
    }
    None
}

#[cfg(feature = "js")]
pub(crate) fn resolve_host_dom_target(
    document: &Document,
    target: &crate::HostEventTarget,
) -> megi_js::JsEventTarget {
    match target {
        crate::HostEventTarget::Window => megi_js::JsEventTarget::Window,
        crate::HostEventTarget::Document => megi_js::JsEventTarget::Document,
        crate::HostEventTarget::FormControl(form_id) => {
            let needle = form_id.to_string();
            find_element_with_attr(document, "data-megi-form-id", Some(&needle)).map_or(
                megi_js::JsEventTarget::Document,
                megi_js::JsEventTarget::Node,
            )
        }
        crate::HostEventTarget::OwningForm(form_id) => {
            let needle = form_id.to_string();
            let Some(control_node) =
                find_element_with_attr(document, "data-megi-form-id", Some(&needle))
            else {
                return megi_js::JsEventTarget::Document;
            };
            std::iter::once(control_node)
                .chain(document.ancestors(control_node))
                .find(|&id| {
                    document
                        .try_node(id)
                        .and_then(|node| node.as_element())
                        .is_some_and(|el| el.local_name.eq_ignore_ascii_case("form"))
                })
                .map_or(
                    megi_js::JsEventTarget::Node(control_node),
                    megi_js::JsEventTarget::Node,
                )
        }
        crate::HostEventTarget::Focused => {
            find_element_with_attr(document, "data-megi-focused", None).map_or(
                megi_js::JsEventTarget::Document,
                megi_js::JsEventTarget::Node,
            )
        }
    }
}

/// Runs due timers until the queue is empty or the task/virtual-time budgets are exhausted.
///
/// Order: pending microtasks are drained before each batch, then every timer due at the
/// current virtual time runs in `(due, id)` order; when nothing is due the clock advances
/// [`VIRTUAL_TIME_STEP_MS`] and the timers that just became due run. The clock never
/// exceeds `max_virtual_time_ms` and the callbacks never exceed `max_tasks`; hitting either
/// cap sets [`EventLoopState::limited`] and leaves the remaining timers pending.
#[cfg(feature = "js")]
pub(crate) fn run_event_loop(
    runtime: &mut megi_js::JsRuntime,
    limits: &ScriptingLimits,
    state: &mut EventLoopState,
    warnings: &mut Vec<String>,
) {
    loop {
        // #1534: deliver any pending WebSocket events before running timers. When sockets are
        // still open but no timer is due, pump briefly so a response can arrive before the
        // render finalizes (bounded to keep the pipeline deterministic and non-hanging).
        runtime.poll_websockets();
        // #1826: frame/postMessage deliveries consume the same task budget as timers,
        // otherwise a self-reposting message handler could run unbounded. Also surface
        // delivery errors instead of silently dropping them.
        match runtime.run_frame_tasks(limits.max_tasks.saturating_sub(state.tasks).min(64)) {
            Ok(delivered) => state.tasks = state.tasks.saturating_add(delivered),
            Err(error) => {
                warnings.push(format!("frame tasks failed: {error}"));
                state.limited = true;
            }
        }
        drain_microtasks(runtime, warnings);
        let remaining = limits.max_tasks.saturating_sub(state.tasks);
        if remaining == 0 {
            if runtime.pending_timers() > 0 {
                state.limited = true;
                warnings.push(format!(
                    "timer task budget exhausted ({} tasks); dropping remaining callbacks",
                    limits.max_tasks
                ));
            }
            return;
        }
        // Only run timers that are due on the *virtual* clock. `run_tasks`
        // follows the wall clock, so on a slow host it can fire a timer before
        // its delay has elapsed on the virtual timeline, making the number of
        // executed callbacks depend on machine speed (this was caught by CI:
        // the same render produced different task counts on different runs).
        match runtime.run_until_with_budget(state.virtual_time_ms, remaining) {
            Ok(0) => {}
            Ok(ran) => {
                state.tasks += ran;
                state.iterations += 1;
                continue;
            }
            Err(error) => {
                warnings.push(format!("timer callback failed: {error}"));
                state.limited = true;
                return;
            }
        }
        if runtime.pending_timers() == 0 {
            if runtime.has_websockets() {
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    runtime.poll_websockets();
                    drain_microtasks(runtime, warnings);
                    if !runtime.has_websockets() {
                        break;
                    }
                }
            }
            return;
        }
        if state.virtual_time_ms.saturating_sub(VIRTUAL_EPOCH_MS) >= limits.max_virtual_time_ms {
            state.limited = true;
            warnings.push(format!(
                "virtual-time budget exhausted ({} ms); dropping {} pending timer(s)",
                limits.max_virtual_time_ms,
                runtime.pending_timers()
            ));
            return;
        }
        let next = state.virtual_time_ms.saturating_add(VIRTUAL_TIME_STEP_MS);
        let remaining = limits.max_tasks.saturating_sub(state.tasks);
        match runtime.run_until_with_budget(next, remaining) {
            Ok(ran) => {
                state.tasks += ran;
                state.iterations += 1;
                // The loop advances the virtual clock by exactly one step, so
                // the timeline does not depend on how long the host took.
                state.virtual_time_ms = next;
            }
            Err(error) => {
                warnings.push(format!("timer callback failed: {error}"));
                state.limited = true;
                return;
            }
        }
    }
}

#[cfg(all(test, feature = "js"))]
mod tests {
    use super::*;

    /// #1826: a self-`postMessage` handler that re-posts must consume the `max_tasks`
    /// budget. Without counting `run_frame_tasks` deliveries, the loop spins delivering
    /// frame messages without ever exhausting the task budget.
    #[test]
    fn frame_messages_consume_the_task_budget() {
        let doc = megi_html::parse_document("<!doctype html><html><body></body></html>", false);
        let mut runtime = megi_js::JsRuntime::with_document(doc, "https://app.example/t".into())
            .expect("runtime");
        runtime
            .eval(
                "window.__n = 0; \
                 window.addEventListener('message', function () { \
                   __n += 1; \
                   if (__n < 1000) { window.postMessage('again', '*'); } \
                 }); \
                 window.postMessage('go', '*');",
                "t",
            )
            .expect("eval");
        let limits = ScriptingLimits {
            max_tasks: 4,
            max_virtual_time_ms: 10_000,
            ..ScriptingLimits::default()
        };
        let mut state = EventLoopState {
            virtual_time_ms: VIRTUAL_EPOCH_MS,
            ..EventLoopState::default()
        };
        let mut warnings = Vec::new();
        run_event_loop(&mut runtime, &limits, &mut state, &mut warnings);
        assert_eq!(
            state.tasks, 4,
            "frame deliveries must count against max_tasks (#1826)"
        );
    }
}
