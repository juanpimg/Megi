use super::*;

pub(crate) fn add_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    // #1948: Web IDL converts null/undefined for the `EventListener?` argument to
    // null, and DOM §2.7 says a null listener does nothing. Keep throwing for
    // non-null primitives, which are not valid EventListener values.
    let listener = args.get(1).cloned().unwrap_or(JsValue::undefined());
    if listener.is_null_or_undefined() {
        return Ok(JsValue::undefined());
    }
    if listener.as_object().is_none() {
        return Err(js_error("listener must be a function"));
    }
    let callback = listener;
    let mut capture = false;
    let mut once = false;
    let mut passive = false;
    const MAX_EVENT_LISTENERS: usize = 16_384;

    if let Some(value) = args.get(2) {
        if let Some(options) = value.as_object() {
            capture = prop(&options, "capture", context)
                .ok()
                .map(|value| value.to_boolean())
                .unwrap_or(false);
            once = prop(&options, "once", context)
                .ok()
                .map(|value| value.to_boolean())
                .unwrap_or(false);
            passive = prop(&options, "passive", context)
                .ok()
                .map(|value| value.to_boolean())
                .unwrap_or(false);
        } else if !value.is_undefined() {
            capture = value.to_boolean();
        }
    }
    // F6 signal option: an already-aborted signal prevents registration, otherwise the
    // listener is tagged so the polyfill's `_abort()` can remove it through the native hook.
    let signal = args
        .get(2)
        .and_then(|value| value.as_object())
        .and_then(|options| prop(&options, "signal", context).ok())
        .and_then(|value| value.as_object());
    if let Some(signal) = &signal {
        let aborted = prop(signal, "aborted", context)
            .ok()
            .and_then(|value| value.as_boolean())
            .unwrap_or(false);
        if aborted {
            return Ok(JsValue::undefined());
        }
    }
    let signal_id = signal
        .as_ref()
        .and_then(|signal| prop(signal, "__megi_signal_id", context).ok())
        .and_then(|id| id.as_number())
        .map(|id| id as u32);

    // DOM §2.7 step 2: adding the same callback with the same capture flag is a no-op.
    let (existing, at_cap): (Vec<u32>, bool) = {
        let host = host_ref(context)?;
        let slots = host
            .listeners
            .iter()
            .filter(|listener| {
                listener.target == target
                    && listener.event_type == event_type
                    && listener.capture == capture
            })
            .map(|listener| listener.slot)
            .collect();
        (slots, host.listeners.len() >= MAX_EVENT_LISTENERS)
    };
    for slot in existing {
        if array_get(context, "__megi_listeners", u64::from(slot))?.strict_equals(&callback) {
            return Ok(JsValue::undefined());
        }
    }
    if at_cap {
        return Err(js_error("event listener limit exceeded"));
    }
    let reused_slot = host_cell(context)?.free_listener_slots.pop();
    let slot = match reused_slot {
        Some(slot) => {
            array_put(context, "__megi_listeners", u64::from(slot), callback)?;
            slot
        }
        None => array_push(context, "__megi_listeners", callback)?,
    };
    host_cell(context)?.listeners.push(Listener {
        target,
        event_type,
        capture,
        once,
        passive,
        slot,
        signal_id,
    });
    Ok(JsValue::undefined())
}

pub(crate) fn remove_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    let Some(callback) = args.get(1).cloned() else {
        return Ok(JsValue::undefined());
    };
    let capture_filter = args.get(2).and_then(|value| {
        if value.is_undefined() {
            None
        } else if let Some(options) = value.as_object() {
            Some(
                prop(&options, "capture", context)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false),
            )
        } else {
            Some(value.to_boolean())
        }
    });
    let candidates: Vec<(usize, u32, bool)> = {
        let host = host_ref(context)?;
        host.listeners
            .iter()
            .enumerate()
            .filter(|(_, listener)| listener.target == target && listener.event_type == event_type)
            .map(|(index, listener)| (index, listener.slot, listener.capture))
            .collect()
    };
    let mut remove_index = None;
    for (index, slot, capture) in candidates {
        if let Some(filter) = capture_filter
            && filter != capture
        {
            continue;
        }
        let stored = array_get(context, "__megi_listeners", u64::from(slot))?;
        if stored.strict_equals(&callback) {
            remove_index = Some(index);
            break;
        }
    }
    if let Some(index) = remove_index {
        let freed_slot = {
            let mut host = host_cell(context)?;
            if index < host.listeners.len() {
                let slot = host.listeners.remove(index).slot;
                host.free_listener_slots.push(slot);
                Some(slot)
            } else {
                None
            }
        };
        if let Some(slot) = freed_slot {
            let _ = array_put(
                context,
                "__megi_listeners",
                u64::from(slot),
                JsValue::undefined(),
            );
        }
    }
    Ok(JsValue::undefined())
}

/// Dispatches a synthesized event of `event_type` on `target` (used by form submission,
/// #1505). Builds a real Event object so listeners and inline handlers observe it normally.
pub(crate) fn dispatch_event_type(
    target: &JsValue,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    context: &mut Context,
) -> JsResult<bool> {
    let event = build_event(
        context,
        event_type,
        bubbles,
        cancelable,
        JsValue::undefined(),
    );
    let args = [event];
    let result = dispatch_event(target, &args, context)?;
    Ok(result.to_boolean())
}

/// Dispatches a cancelable, bubbling `submit` event with an optional `submitter` property (#1538).
pub(crate) fn dispatch_submit_event(
    target: &JsValue,
    submitter: JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let event = build_event(context, "submit", true, true, JsValue::undefined());
    if let Some(obj) = event.as_object() {
        let _ = obj.set(js_string!("submitter"), submitter, false, context);
    }
    let args = [event];
    let result = dispatch_event(target, &args, context)?;
    Ok(result.to_boolean())
}

/// Forwards a dispatched DOM event to workers subscribed through the DOM proxy
/// (F6-WORKERS-DOM), using the existing deterministic worker message pump.
pub(crate) fn forward_dom_event_to_workers(
    target: TargetKey,
    event_type: &str,
    context: &mut Context,
) {
    let node = match target {
        TargetKey::Node(handle) => host_ref(context)
            .ok()
            .and_then(|host| host.nodes.get(handle as usize).copied()),
        _ => None,
    };
    let Some(node) = node else { return };
    // Preserve `(worker_id, token)` so events only reach subscribed workers (#2148).
    let mut deliveries: Vec<(u32, u64)> = Vec::new();
    if let Ok(host) = host_ref(context) {
        for (worker_id, registered_node, registered_type, token) in &host.worker_dom_listeners {
            if *registered_node == node
                && registered_type == event_type
                && !deliveries.contains(&(*worker_id, *token))
            {
                deliveries.push((*worker_id, *token));
            }
        }
    }
    if deliveries.is_empty() {
        return;
    }
    if let Ok(mut host) = host_cell(context) {
        for (worker_id, token) in deliveries {
            let Some(Some(worker)) = host.workers.get_mut(worker_id as usize) else {
                continue;
            };
            // RFC 8259 §7: escape control characters and quotes via `module_quote` (#2179).
            let message = format!(
                "{{\"__megi_dom_event\":{{\"type\":{},\"token\":{}}}}}",
                crate::runtime::module_quote(event_type),
                token
            );
            // #2157: enforce MAX_WORKER_MESSAGES on DOM event forwarding so bursty page
            // dispatches cannot overflow the worker's inbox/outbox.
            let _ = worker.push_to_worker(message);
        }
    }
}

pub(crate) fn dispatch_event(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("dispatchEvent expects an Event"))?;
    if prop(&event, "__megi_event", context)
        .ok()
        .and_then(|value| value.as_boolean())
        != Some(true)
    {
        return Err(js_error("dispatchEvent expects an Event"));
    }
    let event_type = prop(&event, "type", context)?
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))?;
    forward_dom_event_to_workers(target, &event_type, context);
    let bubbles = prop(&event, "bubbles", context)
        .ok()
        .and_then(|value| value.as_boolean())
        .unwrap_or(false);
    // `dispatchEvent` is a method on the target wrapper itself, so `this` is the target.
    let _ = event.set(js_string!("target"), this.clone(), false, context);
    let path = propagation_path(context, target, bubbles)?;
    // Stack the dispatch state so a nested dispatch cannot clobber the outer one (#648).
    let previous_dispatch = host_cell(context)?.dispatch.take();
    let _dispatch_guard = DispatchStateGuard::new(context, previous_dispatch)?;
    host_cell(context)?.dispatch = Some(DispatchState::default());
    let dispatch_result = (|| -> JsResult<bool> {
        for (phase, key) in &path {
            let stopped = host_ref(context)?
                .dispatch
                .map(|dispatch| dispatch.stop_propagation)
                .unwrap_or(false);
            if stopped {
                break;
            }
            deliver(context, *key, *phase, &event_type, &event)?;
            let immediate = host_ref(context)?
                .dispatch
                .map(|dispatch| dispatch.stop_immediate)
                .unwrap_or(false);
            if immediate {
                break;
            }
        }
        Ok(host_ref(context)?
            .dispatch
            .map(|dispatch| dispatch.prevent_default)
            .unwrap_or(false))
    })();
    // These fields must be cleared even when a listener throws or exhausts a budget.
    let _ = event.set(js_string!("currentTarget"), JsValue::null(), false, context);
    let _ = event.set(js_string!("eventPhase"), JsValue::from(0.0), false, context);
    let prevented = dispatch_result?;
    let _ = event.set(
        js_string!("defaultPrevented"),
        JsValue::from(prevented),
        false,
        context,
    );
    Ok(JsValue::from(!prevented))
}

/// Called by the `AbortSignal` polyfill when a signal aborts: removes every listener that
/// was registered with `{ signal }` (F6).
pub(crate) fn remove_signal_listeners(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        let id = id as u32;
        if let Ok(mut host) = host_cell(context) {
            let doomed: Vec<u32> = host
                .listeners
                .iter()
                .filter(|listener| listener.signal_id == Some(id))
                .map(|listener| listener.slot)
                .collect();
            host.listeners
                .retain(|listener| listener.signal_id != Some(id));
            for slot in doomed {
                host.free_listener_slots.push(slot);
            }
        }
    }
    Ok(JsValue::undefined())
}
