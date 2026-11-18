use super::*;

pub(crate) fn add_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    let callback = args
        .get(1)
        .cloned()
        .filter(|value| value.as_object().is_some())
        .ok_or_else(|| js_error("listener must be a function"))?;
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
