//! WP-A: EventTarget/Event/CustomEvent and the timer scheduler.
//!
//! - Event listeners live in the **host** table; callbacks are stored in the realm's
//!   rooted `__megi_listeners` array (Boa `JsValue`s must stay reachable from a GC root).
//! - Dispatch: capture listeners run window→document→ancestors→target, bubble listeners
//!   target→ancestors→document→window. `preventDefault`/`stopPropagation` state lives in
//!   the host for the duration of the dispatch.
//! - Timers: host-side scheduler with `(due_ms, insertion id)` ordering. `setTimeout` and
//!   `setInterval` accept extra arguments; callbacks/arguments are stored in the rooted
//!   `__megi_timer_callbacks`/`__megi_timer_args` arrays. The clock is injectable:
//!   real elapsed time by default, virtual after `JsRuntime::advance`/`run_until`.

use std::time::Duration;

use boa_engine::{
    Context, JsNativeError, JsObject, JsResult, JsValue, NativeFunction, js_string,
    object::FunctionObjectBuilder, object::ObjectInitializer, object::builtins::JsArray,
    property::Attribute,
};
use dom::NodeId;

use crate::error::JsError as CrateJsError;
use crate::host::{DispatchState, Listener, TargetKey, TimerEntry, with_host};

type NativeFn = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

pub(crate) fn js_error(message: impl Into<String>) -> boa_engine::JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

fn native(f: NativeFn) -> NativeFunction {
    NativeFunction::from_fn_ptr(f)
}

fn accessor_fn(context: &mut Context, f: NativeFn) -> boa_engine::object::builtins::JsFunction {
    FunctionObjectBuilder::new(context.realm(), native(f)).build()
}

fn arg_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}

fn prop(object: &JsObject, name: &str, context: &mut Context) -> JsResult<JsValue> {
    object
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))
}

fn global_get(context: &mut Context, name: &str) -> JsResult<JsValue> {
    context
        .global_object()
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))
}

fn global_set(context: &mut Context, name: &str, value: JsValue) {
    let global = context.global_object();
    let _ = global.set(js_string!(name), value, false, context);
}

fn array_push(context: &mut Context, name: &str, value: JsValue) -> JsResult<u32> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Err(js_error("rooted array missing"));
    };
    let length = array
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u32;
    array
        .set(js_string!(length.to_string()), value, false, context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(length)
}

fn array_get(context: &mut Context, name: &str, index: u64) -> JsResult<JsValue> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Ok(JsValue::undefined());
    };
    array
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))
}

fn array_put(context: &mut Context, name: &str, index: u64, value: JsValue) -> JsResult<()> {
    let array = global_get(context, name)?;
    let Some(array) = array.as_object() else {
        return Ok(());
    };
    array
        .set(js_string!(index.to_string()), value, false, context)
        .map(|_| ())
        .map_err(|error| js_error(error.to_string()))
}

/// Installs event constructors, window EventTarget methods and timer globals.
pub(crate) fn install(context: &mut Context) -> Result<(), CrateJsError> {
    global_set(context, "__megi_window", JsValue::from(true));
    for name in [
        "__megi_listeners",
        "__megi_timer_callbacks",
        "__megi_timer_args",
    ] {
        let array = JsArray::new(context);
        global_set(context, name, JsValue::from(array));
    }
    for (name, length, function) in window_methods() {
        context
            .register_global_builtin_callable(js_string!(name), length, native(function))
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    for (name, length, function) in [
        ("Event", 2usize, event_constructor as NativeFn),
        ("CustomEvent", 2, custom_event_constructor),
    ] {
        context
            .register_global_callable(js_string!(name), length, native(function))
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    for (name, length, function) in timer_globals() {
        context
            .register_global_builtin_callable(js_string!(name), length, native(function))
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn window_methods() -> [(&'static str, usize, NativeFn); 3] {
    [
        ("addEventListener", 2, add_listener),
        ("removeEventListener", 2, remove_listener),
        ("dispatchEvent", 1, dispatch_event),
    ]
}

pub(crate) fn node_methods() -> [(&'static str, usize, NativeFn); 3] {
    window_methods()
}

fn target_key(this: &JsValue, context: &mut Context) -> JsResult<TargetKey> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("receiver is not an event target"))?;
    if let Ok(handle) = prop(&object, "__megi_node", context)
        && let Some(handle) = handle.as_number()
    {
        return Ok(TargetKey::Node(handle as u32));
    }
    if prop(&object, "__megi_window", context)
        .ok()
        .and_then(|value| value.as_boolean())
        == Some(true)
    {
        return Ok(TargetKey::Window);
    }
    if prop(&object, "__megi_document", context)
        .ok()
        .and_then(|value| value.as_boolean())
        == Some(true)
    {
        return Ok(TargetKey::Document);
    }
    Err(js_error("receiver is not an event target"))
}

fn node_id_of(handle: u32) -> JsResult<NodeId> {
    with_host(|host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

// ---------------------------------------------------------------------------
// Event objects
// ---------------------------------------------------------------------------

fn build_event(
    context: &mut Context,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    detail: JsValue,
) -> JsValue {
    let prevent = accessor_fn(context, event_prevent_default);
    let stop = accessor_fn(context, event_stop_propagation);
    let stop_immediate = accessor_fn(context, event_stop_immediate);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_event"), true, Attribute::all())
        .property(js_string!("type"), js_string!(event_type), Attribute::all())
        .property(js_string!("bubbles"), bubbles, Attribute::all())
        .property(js_string!("cancelable"), cancelable, Attribute::all())
        .property(js_string!("detail"), detail, Attribute::all())
        .property(js_string!("target"), JsValue::null(), Attribute::all())
        .property(
            js_string!("currentTarget"),
            JsValue::null(),
            Attribute::all(),
        )
        .property(js_string!("defaultPrevented"), false, Attribute::all())
        .property(js_string!("eventPhase"), 0.0, Attribute::all())
        .function(
            native(event_prevent_default),
            js_string!("preventDefault"),
            0,
        )
        .function(
            native(event_stop_propagation),
            js_string!("stopPropagation"),
            0,
        )
        .function(
            native(event_stop_immediate),
            js_string!("stopImmediatePropagation"),
            0,
        );
    let _ = (prevent, stop, stop_immediate);
    JsValue::from(init.build())
}

fn event_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let event_type = arg_string(args, 0, context)?;
    let options = args.get(1).and_then(|value| value.as_object());
    let bubbles = options
        .as_ref()
        .and_then(|options| {
            prop(options, "bubbles", context)
                .ok()
                .and_then(|v| v.as_boolean())
        })
        .unwrap_or(false);
    let cancelable = options
        .as_ref()
        .and_then(|options| {
            prop(options, "cancelable", context)
                .ok()
                .and_then(|v| v.as_boolean())
        })
        .unwrap_or(false);
    Ok(build_event(
        context,
        &event_type,
        bubbles,
        cancelable,
        JsValue::undefined(),
    ))
}

fn custom_event_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let event_type = arg_string(args, 0, context)?;
    let options = args.get(1).and_then(|value| value.as_object());
    let detail = options
        .as_ref()
        .map(|options| prop(options, "detail", context).unwrap_or_else(|_| JsValue::undefined()))
        .unwrap_or_else(JsValue::undefined);
    let bubbles = options
        .as_ref()
        .and_then(|options| {
            prop(options, "bubbles", context)
                .ok()
                .and_then(|v| v.as_boolean())
        })
        .unwrap_or(false);
    Ok(build_event(context, &event_type, bubbles, true, detail))
}

fn check_event(this: &JsValue, context: &mut Context) -> JsResult<JsObject> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("receiver is not an Event"))?;
    if prop(&object, "__megi_event", context)
        .ok()
        .and_then(|value| value.as_boolean())
        == Some(true)
    {
        Ok(object)
    } else {
        Err(js_error("receiver is not an Event"))
    }
}

fn event_prevent_default(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    let _ = with_host(|host| {
        if let Some(dispatch) = &mut host.dispatch {
            dispatch.prevent_default = true;
        }
    });
    Ok(JsValue::undefined())
}

fn event_stop_propagation(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    let _ = with_host(|host| {
        if let Some(dispatch) = &mut host.dispatch {
            dispatch.stop_propagation = true;
        }
    });
    Ok(JsValue::undefined())
}

fn event_stop_immediate(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    let _ = with_host(|host| {
        if let Some(dispatch) = &mut host.dispatch {
            dispatch.stop_immediate = true;
        }
    });
    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// EventTarget methods
// ---------------------------------------------------------------------------

fn add_listener(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    let callback = args
        .get(1)
        .cloned()
        .filter(|value| value.as_object().is_some())
        .ok_or_else(|| js_error("listener must be a function"))?;
    let capture = match args.get(2) {
        Some(value) => {
            if let Some(flag) = value.as_boolean() {
                flag
            } else {
                value
                    .as_object()
                    .map(|options| {
                        prop(&options, "capture", context)
                            .ok()
                            .and_then(|value| value.as_boolean())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            }
        }
        None => false,
    };
    let slot = array_push(context, "__megi_listeners", callback)?;
    let _ = with_host(|host| {
        host.listeners.push(Listener {
            target,
            event_type,
            capture,
            slot,
        });
    });
    Ok(JsValue::undefined())
}

fn remove_listener(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    let _ = with_host(|host| {
        host.listeners
            .retain(|listener| !(listener.target == target && listener.event_type == event_type));
    });
    Ok(JsValue::undefined())
}

fn dispatch_event(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
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
    let path = propagation_path(target, bubbles);
    let _ = with_host(|host| host.dispatch = Some(DispatchState::default()));
    for (capture_phase, key) in &path {
        let stopped = with_host(|host| host.dispatch.map(|dispatch| dispatch.stop_propagation))
            .flatten()
            .unwrap_or(false);
        if stopped {
            break;
        }
        deliver(context, *key, *capture_phase, &event_type, &event)?;
        let immediate = with_host(|host| host.dispatch.map(|dispatch| dispatch.stop_immediate))
            .flatten()
            .unwrap_or(false);
        if immediate {
            break;
        }
    }
    let prevented = with_host(|host| host.dispatch.map(|dispatch| dispatch.prevent_default))
        .flatten()
        .unwrap_or(false);
    let _ = with_host(|host| host.dispatch = None);
    let _ = event.set(
        js_string!("defaultPrevented"),
        JsValue::from(prevented),
        false,
        context,
    );
    Ok(JsValue::from(!prevented))
}

fn propagation_path(target: TargetKey, bubbles: bool) -> Vec<(bool, TargetKey)> {
    let mut ancestors: Vec<TargetKey> = Vec::new();
    if let TargetKey::Node(handle) = target
        && let Ok(node) = node_id_of(handle)
    {
        let chain = with_host(|host| {
            let mut ids = Vec::new();
            let mut current = Some(node);
            while let Some(id) = current {
                ids.push(id);
                current = host.document.try_node(id).and_then(|node| node.parent);
            }
            ids
        })
        .unwrap_or_default();
        for id in chain.iter().skip(1) {
            if let Some(handle) =
                with_host(|host| host.handle_for(*id)).and_then(|result| result.ok())
            {
                ancestors.push(TargetKey::Node(handle));
            }
        }
    }
    let mut path = Vec::new();
    path.push((true, TargetKey::Window));
    path.push((true, TargetKey::Document));
    for ancestor in ancestors.iter().rev() {
        path.push((true, *ancestor));
    }
    path.push((false, target));
    if bubbles {
        for ancestor in &ancestors {
            path.push((false, *ancestor));
        }
        path.push((false, TargetKey::Document));
        path.push((false, TargetKey::Window));
    }
    path
}

fn deliver(
    context: &mut Context,
    target: TargetKey,
    capture_phase: bool,
    event_type: &str,
    event: &JsObject,
) -> JsResult<()> {
    let slots: Vec<u32> = with_host(|host| {
        host.listeners
            .iter()
            .filter(|listener| {
                listener.target == target
                    && listener.event_type == event_type
                    && listener.capture == capture_phase
            })
            .map(|listener| listener.slot)
            .collect()
    })
    .unwrap_or_default();
    if slots.is_empty() {
        return Ok(());
    }
    let _ = event.set(js_string!("currentTarget"), JsValue::null(), false, context);
    for slot in slots {
        let callback = array_get(context, "__megi_listeners", u64::from(slot))?;
        if let Some(callback) = callback.as_object() {
            callback
                .call(
                    &JsValue::undefined(),
                    &[JsValue::from(event.clone())],
                    context,
                )
                .map_err(|error| js_error(error.to_string()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

pub(crate) fn timer_globals() -> [(&'static str, usize, NativeFn); 4] {
    [
        ("setTimeout", 2, set_timeout),
        ("setInterval", 2, set_interval),
        ("clearTimeout", 1, clear_timer),
        ("clearInterval", 1, clear_timer),
    ]
}

fn schedule_timer(args: &[JsValue], context: &mut Context, repeat: bool) -> JsResult<JsValue> {
    let callback = args
        .first()
        .cloned()
        .filter(|value| value.as_object().is_some())
        .ok_or_else(|| js_error("timer callback must be a function"))?;
    let delay = args
        .get(1)
        .and_then(JsValue::as_number)
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let id = with_host(|host| {
        let id = host.next_timer_id;
        host.next_timer_id += 1;
        let now = host.now_ms();
        host.timers.push(TimerEntry {
            id,
            due_ms: now + delay,
            interval_ms: repeat.then_some(delay.max(1)),
        });
        id
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    array_put(context, "__megi_timer_callbacks", id, callback)?;
    let args_array = JsArray::new(context);
    for argument in args.iter().skip(2) {
        let _ = args_array.push(argument.clone(), context);
    }
    array_put(context, "__megi_timer_args", id, JsValue::from(args_array))?;
    Ok(JsValue::from(id as f64))
}

fn set_timeout(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    schedule_timer(args, context, false)
}

fn set_interval(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    schedule_timer(args, context, true)
}

fn clear_timer(_this: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        let id = id.max(0.0) as u64;
        let _ = with_host(|host| host.timers.retain(|timer| timer.id != id));
    }
    Ok(JsValue::undefined())
}

/// Runs due timers up to `deadline_ms` in `(due, id)` order; returns how many ran.
pub(crate) fn run_due_timers(
    context: &mut Context,
    deadline_ms: u64,
    max_steps: usize,
) -> Result<usize, CrateJsError> {
    let mut executed = 0usize;
    while executed < max_steps {
        let next = with_host(|host| {
            let effective = host.now_ms().max(deadline_ms);
            host.timers
                .iter()
                .enumerate()
                .filter(|(_, timer)| timer.due_ms <= effective)
                .min_by_key(|(_, timer)| (timer.due_ms, timer.id))
                .map(|(index, timer)| (index, timer.id, timer.interval_ms, timer.due_ms))
        })
        .flatten();
        let Some((index, id, interval, due_ms)) = next else {
            break;
        };
        let _ = with_host(|host| {
            if let Some(interval) = interval {
                if let Some(timer) = host.timers.get_mut(index) {
                    timer.due_ms = due_ms + interval;
                }
            } else {
                host.timers.remove(index);
            }
        });
        let callback = array_get(context, "__megi_timer_callbacks", id)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
        if let Some(callback) = callback.as_object() {
            let arguments = array_get(context, "__megi_timer_args", id)
                .map_err(|error| CrateJsError::Runtime(error.to_string()))?
                .as_object()
                .map(|array| {
                    let length = array
                        .get(js_string!("length"), context)
                        .ok()
                        .and_then(|value| value.as_number())
                        .unwrap_or(0.0) as u64;
                    (0..length)
                        .filter_map(|index| array.get(js_string!(index.to_string()), context).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            callback
                .call(&JsValue::undefined(), &arguments, context)
                .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
        }
        executed += 1;
    }
    let _ = with_host(|host| host.set_clock(deadline_ms));
    Ok(executed)
}

/// Advances the virtual clock and runs due timers.
pub(crate) fn advance_clock(context: &mut Context, delta: Duration) -> Result<u64, CrateJsError> {
    let now = with_host(|host| host.advance_clock(delta))
        .ok_or_else(|| CrateJsError::Internal("no active JavaScript host".to_string()))?;
    let _ = run_due_timers(context, now, 100_000)?;
    Ok(now)
}

/// Sets the virtual clock to `deadline_ms` and runs due timers.
pub(crate) fn run_until(context: &mut Context, deadline_ms: u64) -> Result<usize, CrateJsError> {
    let effective = with_host(|host| host.set_clock(deadline_ms))
        .ok_or_else(|| CrateJsError::Internal("no active JavaScript host".to_string()))?;
    run_due_timers(context, effective, 100_000)
}

/// Current scheduler time (virtual or real).
pub(crate) fn now_ms() -> u64 {
    with_host(|host| host.now_ms()).unwrap_or(0)
}
