use super::*;

/// Maximum pending timers and animation frames (anti-DoS cap; browsers also bound them
/// indirectly through nesting/frequency limits).
pub(crate) const MAX_PENDING_TIMERS: usize = 4096;

pub(crate) fn timer_globals() -> [(&'static str, usize, NativeFn); 4] {
    [
        ("setTimeout", 2, set_timeout),
        ("setInterval", 2, set_interval),
        ("clearTimeout", 1, clear_timer),
        ("clearInterval", 1, clear_timer),
    ]
}

pub(crate) fn schedule_timer(
    args: &[JsValue],
    context: &mut Context,
    repeat: bool,
    animation: bool,
) -> JsResult<JsValue> {
    let callback = args
        .first()
        .cloned()
        .filter(|value| value.as_object().is_some())
        .ok_or_else(|| js_error("timer callback must be a function"))?;
    let delay = if animation {
        // `requestAnimationFrame` is queued on the host scheduler with a fixed 16 ms
        // frame budget so render's deterministic event loop can pump it.
        16
    } else {
        // #1622: apply the ECMAScript ToNumber coercion (setTimeout's delay is `long`, so a
        // numeric string like "100" must schedule 100ms, not fall back to an immediate 0ms).
        let raw = args
            .get(1)
            .map(|value| value.to_number(context).unwrap_or(f64::NAN))
            .unwrap_or(0.0);
        (if raw.is_finite() { raw.max(0.0) } else { 0.0 }) as u64
    };
    let id = {
        let mut host = host_cell(context)?;
        if host.timers.len() >= MAX_PENDING_TIMERS {
            return Err(js_error("too many pending timers"));
        }
        let id = host.next_timer_id;
        host.next_timer_id += 1;
        let now = host.now_ms();
        host.timers.push(TimerEntry {
            id,
            due_ms: now.saturating_add(delay),
            interval_ms: repeat.then_some(delay.max(1)),
            animation,
        });
        id
    };
    array_put(context, "__megi_timer_callbacks", id, callback)?;
    let args_array = JsArray::from_iter([], context);
    for argument in args.iter().skip(2) {
        let _ = args_array.push(argument.clone(), context);
    }
    array_put(context, "__megi_timer_args", id, JsValue::from(args_array))?;
    Ok(JsValue::from(id as f64))
}

pub(crate) fn set_timeout(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    schedule_timer(args, context, false, false)
}

pub(crate) fn set_interval(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    schedule_timer(args, context, true, false)
}

/// `requestAnimationFrame(callback)` — scheduled 16 ms ahead on the injected clock.
pub(crate) fn request_animation_frame(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    schedule_timer(args, context, false, true)
}

/// `cancelAnimationFrame(id)`.
pub(crate) fn cancel_animation_frame(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        if !id.is_finite() || id < 0.0 || id > u64::MAX as f64 {
            return Ok(JsValue::undefined());
        }
        let id = id as u64;
        // #1849: animation frames are a separate timer family (WHATWG HTML §8.1.7);
        // cancelAnimationFrame must not remove `setTimeout`/`setInterval` timers.
        let before = host_cell(context)?.timers.len();
        host_cell(context)?
            .timers
            .retain(|timer| timer.id != id || !timer.animation);
        if host_cell(context)?.timers.len() < before {
            release_timer_slots(context, id);
        }
    }
    Ok(JsValue::undefined())
}

pub(crate) fn clear_timer(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        if !id.is_finite() || id < 0.0 || id > u64::MAX as f64 {
            return Ok(JsValue::undefined());
        }
        let id = id as u64;
        // #1849: clearTimeout clears only the timeout/interval family (WHATWG HTML §8.1.6).
        let before = host_cell(context)?.timers.len();
        host_cell(context)?
            .timers
            .retain(|timer| timer.id != id || timer.animation);
        if host_cell(context)?.timers.len() < before {
            release_timer_slots(context, id);
        }
    }
    Ok(JsValue::undefined())
}

/// Drops the rooted callback/arguments of a timer so completed/cancelled timers do not keep
/// their closures alive in the `__megi_timer_*` arrays.
pub(crate) fn release_timer_slots(context: &mut Context, id: u64) {
    let _ = array_put(context, "__megi_timer_callbacks", id, JsValue::undefined());
    let _ = array_put(context, "__megi_timer_args", id, JsValue::undefined());
}

/// Runs due timers up to `deadline_ms` in `(due, id)` order; returns how many ran.
pub(crate) fn run_due_timers(
    context: &mut Context,
    deadline_ms: u64,
    max_steps: usize,
) -> Result<usize, CrateJsError> {
    let mut executed = 0usize;
    while executed < max_steps {
        let next = {
            let host =
                host_ref(context).map_err(|error| CrateJsError::Runtime(error.to_string()))?;
            let effective = host.now_ms().max(deadline_ms);
            host.timers
                .iter()
                .enumerate()
                .filter(|(_, timer)| timer.due_ms <= effective)
                .min_by_key(|(_, timer)| (timer.due_ms, timer.id))
                .map(|(index, timer)| {
                    (
                        index,
                        timer.id,
                        timer.interval_ms,
                        timer.due_ms,
                        timer.animation,
                    )
                })
        };
        let Some((index, id, interval, due_ms, animation)) = next else {
            break;
        };
        {
            let mut host =
                host_cell(context).map_err(|error| CrateJsError::Runtime(error.to_string()))?;
            if let Some(interval) = interval {
                if let Some(timer) = host.timers.get_mut(index) {
                    timer.due_ms = due_ms.saturating_add(interval);
                }
            } else {
                host.timers.remove(index);
            }
        }
        let callback = array_get(context, "__megi_timer_callbacks", id)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
        if let Some(callback) = callback.as_object() {
            let timestamp = host_ref(context)
                .map_err(|error| CrateJsError::Runtime(error.to_string()))?
                .now_ms();
            let arguments = if animation {
                // rAF callbacks receive the virtual timestamp in milliseconds.
                vec![JsValue::from(timestamp as f64)]
            } else {
                array_get(context, "__megi_timer_args", id)
                    .map_err(|error| CrateJsError::Runtime(error.to_string()))?
                    .as_object()
                    .map(|array| {
                        let length = array
                            .get(js_string!("length"), context)
                            .ok()
                            .and_then(|value| value.as_number())
                            .unwrap_or(0.0) as u64;
                        (0..length)
                            .filter_map(|index| {
                                array.get(js_string!(index.to_string()), context).ok()
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            let call_result = callback.call(&JsValue::undefined(), &arguments, context);
            if interval.is_none() {
                release_timer_slots(context, id);
            }
            call_result.map_err(|error| CrateJsError::Runtime(error.to_string()))?;
        } else if interval.is_none() {
            release_timer_slots(context, id);
        }
        executed += 1;
    }
    host_cell(context)
        .map_err(|error| CrateJsError::Runtime(error.to_string()))?
        .set_clock(deadline_ms);
    refresh_clock_snapshot(context);
    Ok(executed)
}

/// Advances the virtual clock and runs due timers.
pub(crate) fn advance_clock(context: &mut Context, delta: Duration) -> Result<u64, CrateJsError> {
    advance_clock_pumping(context, delta, 100_000).map(|(now, _)| now)
}

/// [`advance_clock`] that runs at most `max_steps` timers and reports how many ran.
pub(crate) fn advance_clock_pumping(
    context: &mut Context,
    delta: Duration,
    max_steps: usize,
) -> Result<(u64, usize), CrateJsError> {
    let now = host_cell(context)
        .map_err(|error| CrateJsError::Internal(error.to_string()))?
        .advance_clock(delta);
    refresh_clock_snapshot(context);
    let executed = run_due_timers(context, now, max_steps)?;
    Ok((now, executed))
}

/// Sets the virtual clock to `deadline_ms` and runs due timers, at most `max_steps` of them.
pub(crate) fn run_until(
    context: &mut Context,
    deadline_ms: u64,
    max_steps: usize,
) -> Result<usize, CrateJsError> {
    let effective = host_cell(context)
        .map_err(|error| CrateJsError::Internal(error.to_string()))?
        .set_clock(deadline_ms);
    refresh_clock_snapshot(context);
    run_due_timers(context, effective, max_steps)
}

/// Current scheduler time (virtual or real).
///
/// Compatibility helper for callers that do not carry a `Context` (the runtime's
/// task pump). Event bindings themselves read the clock from the host attached
/// to the `Context` they already receive.
pub(crate) fn now_ms() -> u64 {
    // TLS_PENDING(crates/js/src/events.rs:1988): the no-`Context` shim reads a clock
    // snapshot refreshed by the context-aware helpers above. The legacy thread-local
    // host bridge is gone; the clean fix is for runtime.rs callers to pass a `Context`
    // (or for host.rs to expose a `host_from_context` helper) and drop this shim.
    CLOCK_SNAPSHOT.with(|cell| cell.get())
}
