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
    object::FunctionObjectBuilder,
    object::ObjectInitializer,
    object::builtins::JsArray,
    property::{Attribute, PropertyDescriptor},
};
use dom::NodeId;

use crate::error::JsError as CrateJsError;
use crate::host::{DispatchState, InlineHandler, Listener, TargetKey, TimerEntry, with_host};

type NativeFn = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

/// Target of a synthetic event dispatched from Rust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsEventTarget {
    /// The `window` global (bubbles only within window/document capture path).
    Window,
    /// The `document` object; with `bubbles` the event continues to `window`.
    Document,
    /// A DOM node, bubbling through its ancestors when requested.
    Node(dom::NodeId),
}

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
        "__megi_wrappers",
        "__megi_inline_handlers",
    ] {
        let array = JsArray::new(context);
        global_set(context, name, JsValue::from(array));
    }
    for (name, length, function) in window_methods() {
        context
            .register_global_builtin_callable(js_string!(name), length, native(function))
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    for (name, get, set) in inline_handler_accessors() {
        let get = accessor_fn(context, get);
        let set = accessor_fn(context, set);
        let descriptor = PropertyDescriptor::builder()
            .get(JsValue::from(get))
            .set(JsValue::from(set))
            .enumerable(true)
            .configurable(true)
            .build();
        context
            .global_object()
            .define_property_or_throw(js_string!(name), descriptor, context)
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
    for (name, length, function) in [
        (
            "requestAnimationFrame",
            1usize,
            request_animation_frame as NativeFn,
        ),
        ("cancelAnimationFrame", 1, cancel_animation_frame),
    ] {
        context
            .register_global_builtin_callable(js_string!(name), length, native(function))
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    // Viewport/scroll read-only accessors on the `window` global.
    for (name, get) in [
        ("innerWidth", window_inner_width as NativeFn),
        ("innerHeight", window_inner_height),
        ("devicePixelRatio", window_device_pixel_ratio),
        ("pageXOffset", window_page_x),
        ("pageYOffset", window_page_y),
    ] {
        let getter = accessor_fn(context, get);
        let descriptor = PropertyDescriptor::builder()
            .get(JsValue::from(getter))
            .enumerable(true)
            .configurable(true)
            .build();
        context
            .global_object()
            .define_property_or_throw(js_string!(name), descriptor, context)
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn window_methods() -> [(&'static str, usize, NativeFn); 7] {
    [
        ("addEventListener", 2, add_listener),
        ("removeEventListener", 2, remove_listener),
        ("dispatchEvent", 1, dispatch_event),
        ("getComputedStyle", 1, get_computed_style),
        ("matchMedia", 1, match_media),
        ("scrollTo", 2, scroll_to),
        ("scrollBy", 2, scroll_by),
    ]
}

pub(crate) fn node_methods() -> [(&'static str, usize, NativeFn); 3] {
    [
        ("addEventListener", 2, add_listener),
        ("removeEventListener", 2, remove_listener),
        ("dispatchEvent", 1, dispatch_event),
    ]
}

// ---------------------------------------------------------------------------
// Inline handlers (`onclick`, `onload`, `onerror`)
// ---------------------------------------------------------------------------

fn inline_get(this: &JsValue, event_type: &str, context: &mut Context) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let slot = with_host(|host| {
        host.inline_handlers
            .iter()
            .find(|handler| handler.target == target && handler.event_type == event_type)
            .map(|handler| handler.slot)
    })
    .flatten();
    match slot {
        Some(slot) => array_get(context, "__megi_inline_handlers", u64::from(slot)),
        None => Ok(JsValue::null()),
    }
}

fn inline_set(
    this: &JsValue,
    event_type: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    if value.as_object().is_none() {
        let _ = with_host(|host| {
            host.inline_handlers
                .retain(|handler| !(handler.target == target && handler.event_type == event_type));
        });
        return Ok(JsValue::undefined());
    }
    let slot = array_push(context, "__megi_inline_handlers", value)?;
    let _ = with_host(|host| {
        host.inline_handlers
            .retain(|handler| !(handler.target == target && handler.event_type == event_type));
        host.inline_handlers.push(InlineHandler {
            target,
            event_type: event_type.to_string(),
            slot,
        });
    });
    Ok(JsValue::undefined())
}

macro_rules! inline_handler_accessors {
    ($(($get:ident, $set:ident, $property:literal, $event:literal)),+ $(,)?) => {
        $(
            fn $get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                inline_get(this, $event, context)
            }

            fn $set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                inline_set(this, $event, args, context)
            }
        )+

        /// Name/getter/setter triples for `window.onclick`, `onload` and `onerror`.
        pub(crate) fn inline_handler_accessors() -> Vec<(&'static str, NativeFn, NativeFn)> {
            vec![$(($property, $get as NativeFn, $set as NativeFn)),+]
        }
    };
}

inline_handler_accessors!(
    (inline_onclick_get, inline_onclick_set, "onclick", "click"),
    (inline_onload_get, inline_onload_set, "onload", "load"),
    (inline_onerror_get, inline_onerror_set, "onerror", "error"),
);

/// Inline handler accessor pairs shared with node/document wrappers.
pub(crate) fn inline_node_accessors() -> [(&'static str, NativeFn, NativeFn); 3] {
    [
        ("onclick", inline_onclick_get, inline_onclick_set),
        ("onload", inline_onload_get, inline_onload_set),
        ("onerror", inline_onerror_get, inline_onerror_set),
    ]
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
            // `{ passive: true }` listeners cannot cancel the default action.
            if !dispatch.passive {
                dispatch.prevent_default = true;
            }
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
    let mut capture = false;
    let mut once = false;
    let mut passive = false;
    if let Some(value) = args.get(2) {
        if let Some(flag) = value.as_boolean() {
            capture = flag;
        } else if let Some(options) = value.as_object() {
            capture = prop(&options, "capture", context)
                .ok()
                .and_then(|value| value.as_boolean())
                .unwrap_or(false);
            once = prop(&options, "once", context)
                .ok()
                .and_then(|value| value.as_boolean())
                .unwrap_or(false);
            passive = prop(&options, "passive", context)
                .ok()
                .and_then(|value| value.as_boolean())
                .unwrap_or(false);
        }
    }
    // DOM §2.7 step 2: adding the same callback with the same capture flag is a no-op.
    let existing: Vec<u32> = with_host(|host| {
        host.listeners
            .iter()
            .filter(|listener| {
                listener.target == target
                    && listener.event_type == event_type
                    && listener.capture == capture
            })
            .map(|listener| listener.slot)
            .collect()
    })
    .unwrap_or_default();
    for slot in existing {
        if array_get(context, "__megi_listeners", u64::from(slot))?.strict_equals(&callback) {
            return Ok(JsValue::undefined());
        }
    }
    let slot = array_push(context, "__megi_listeners", callback)?;
    let _ = with_host(|host| {
        host.listeners.push(Listener {
            target,
            event_type,
            capture,
            once,
            passive,
            slot,
        });
    });
    Ok(JsValue::undefined())
}

fn remove_listener(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    let Some(callback) = args.get(1).cloned() else {
        return Ok(JsValue::undefined());
    };
    let capture_filter = args.get(2).and_then(|value| value.as_boolean());
    let candidates: Vec<(usize, u32, bool)> = with_host(|host| {
        host.listeners
            .iter()
            .enumerate()
            .filter(|(_, listener)| listener.target == target && listener.event_type == event_type)
            .map(|(index, listener)| (index, listener.slot, listener.capture))
            .collect()
    })
    .unwrap_or_default();
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
        let _ = with_host(|host| {
            if index < host.listeners.len() {
                host.listeners.remove(index);
            }
        });
    }
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
    for (phase, key) in &path {
        let stopped = with_host(|host| host.dispatch.map(|dispatch| dispatch.stop_propagation))
            .flatten()
            .unwrap_or(false);
        if stopped {
            break;
        }
        deliver(context, *key, *phase, &event_type, &event)?;
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
    let _ = event.set(js_string!("currentTarget"), JsValue::null(), false, context);
    let _ = event.set(js_string!("eventPhase"), JsValue::from(0.0), false, context);
    let _ = event.set(
        js_string!("defaultPrevented"),
        JsValue::from(prevented),
        false,
        context,
    );
    Ok(JsValue::from(!prevented))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PropPhase {
    Capture,
    Target,
    Bubble,
}

fn phase_number(phase: PropPhase) -> f64 {
    match phase {
        PropPhase::Capture => 1.0,
        PropPhase::Target => 2.0,
        PropPhase::Bubble => 3.0,
    }
}

fn is_budget_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("loop iteration limit")
        || lower.contains("runtime limit")
        || lower.contains("recursion limit")
        || lower.contains("stack size")
}

fn propagation_path(target: TargetKey, bubbles: bool) -> Vec<(PropPhase, TargetKey)> {
    // Dispatch on `window`: the window is the target (`AT_TARGET`, phase 2), with capture
    // and bubble listeners both running there; there are no ancestors above it.
    if target == TargetKey::Window {
        return vec![(PropPhase::Target, TargetKey::Window)];
    }
    // Dispatch on `document`: window is the only capture ancestor; document listeners run
    // at the target (phase 2), then a single bubble leg to window when requested.
    if target == TargetKey::Document {
        let mut path = vec![
            (PropPhase::Capture, TargetKey::Window),
            (PropPhase::Target, TargetKey::Document),
        ];
        if bubbles {
            path.push((PropPhase::Bubble, TargetKey::Window));
        }
        return path;
    }
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
    // Capture ancestors above the target, top-down. The target itself appears exactly once:
    // at-target listeners run in the AT_TARGET phase regardless of their capture flag.
    if !matches!(target, TargetKey::Window) {
        path.push((PropPhase::Capture, TargetKey::Window));
        if !matches!(target, TargetKey::Document) {
            path.push((PropPhase::Capture, TargetKey::Document));
        }
    }
    for ancestor in ancestors.iter().rev() {
        path.push((PropPhase::Capture, *ancestor));
    }
    path.push((PropPhase::Target, target));
    if bubbles {
        for ancestor in &ancestors {
            path.push((PropPhase::Bubble, *ancestor));
        }
        if !matches!(target, TargetKey::Document) {
            path.push((PropPhase::Bubble, TargetKey::Document));
        }
        if !matches!(target, TargetKey::Window) {
            path.push((PropPhase::Bubble, TargetKey::Window));
        }
    }
    path
}

/// JS value used as `event.currentTarget`/`this` while visiting `target`.
fn target_js_value(context: &mut Context, target: TargetKey) -> JsResult<JsValue> {
    match target {
        TargetKey::Window => Ok(JsValue::from(context.global_object())),
        TargetKey::Document => global_get(context, "document"),
        TargetKey::Node(handle) => crate::runtime::wrap_handle_public(context, handle)
            .map_err(|error| js_error(error.to_string())),
    }
}

/// Invokes one listener, isolating ordinary exceptions (budget errors still propagate).
fn call_listener(
    context: &mut Context,
    callback: &JsObject,
    current_target: &JsValue,
    event: &JsObject,
    passive: bool,
    once: bool,
    slot: u32,
) -> JsResult<()> {
    let _ = with_host(|host| {
        if let Some(dispatch) = &mut host.dispatch {
            dispatch.passive = passive;
        }
    });
    let result = callback.call(current_target, &[JsValue::from(event.clone())], context);
    let _ = with_host(|host| {
        if let Some(dispatch) = &mut host.dispatch {
            dispatch.passive = false;
        }
    });
    if once {
        let _ = with_host(|host| host.listeners.retain(|listener| listener.slot != slot));
    }
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            if is_budget_message(&message) {
                return Err(js_error(message));
            }
            let _ = with_host(|host| {
                host.last_event_error = Some(message.clone());
                host.console.push(crate::host::ConsoleEntry {
                    level: "error".to_string(),
                    message,
                });
            });
            Ok(())
        }
    }
}

fn deliver(
    context: &mut Context,
    target: TargetKey,
    phase: PropPhase,
    event_type: &str,
    event: &JsObject,
) -> JsResult<()> {
    let current_target = target_js_value(context, target)?;
    let _ = event.set(
        js_string!("currentTarget"),
        current_target.clone(),
        false,
        context,
    );
    let _ = event.set(
        js_string!("eventPhase"),
        JsValue::from(phase_number(phase)),
        false,
        context,
    );
    let at_target = phase == PropPhase::Target;
    let listeners: Vec<(u32, bool, bool)> = with_host(|host| {
        let mut matching: Vec<(u32, bool, bool)> = host
            .listeners
            .iter()
            .filter(|listener| listener.target == target && listener.event_type == event_type)
            .filter(|listener| at_target || listener.capture == (phase == PropPhase::Capture))
            .map(|listener| (listener.slot, listener.once, listener.passive))
            .collect();
        if at_target {
            // AT_TARGET: capture listeners first, then bubble listeners (same phase).
            matching.sort_by_key(|(slot, _, _)| {
                !host
                    .listeners
                    .iter()
                    .any(|listener| listener.slot == *slot && listener.capture)
            });
        }
        matching
    })
    .unwrap_or_default();
    for (slot, once, passive) in listeners {
        let immediate = with_host(|host| host.dispatch.map(|dispatch| dispatch.stop_immediate))
            .flatten()
            .unwrap_or(false);
        if immediate {
            return Ok(());
        }
        // A listener removed by an earlier callback in this dispatch must not run.
        let registered =
            with_host(|host| host.listeners.iter().any(|listener| listener.slot == slot))
                .unwrap_or(false);
        if !registered {
            continue;
        }
        let callback = array_get(context, "__megi_listeners", u64::from(slot))?;
        if let Some(callback) = callback.as_object() {
            call_listener(
                context,
                &callback,
                &current_target,
                event,
                passive,
                once,
                slot,
            )?;
        }
    }
    // Inline `on<event>` handlers run at the target (bubble phase), after regular listeners.
    if phase == PropPhase::Target {
        let inline_slot = with_host(|host| {
            host.inline_handlers
                .iter()
                .find(|handler| handler.target == target && handler.event_type == event_type)
                .map(|handler| handler.slot)
        })
        .flatten();
        if let Some(slot) = inline_slot {
            let callback = array_get(context, "__megi_inline_handlers", u64::from(slot))?;
            if let Some(callback) = callback.as_object() {
                call_listener(
                    context,
                    &callback,
                    &current_target,
                    event,
                    false,
                    false,
                    slot,
                )?;
                let returned_false =
                    with_host(|host| host.dispatch.is_some()).unwrap_or(false) && false;
                let _ = returned_false;
            }
        }
    }
    Ok(())
}

/// Dispatches a synthetic event from Rust to the current realm's listeners.
///
/// `target` selects `window`, the `document` object or a DOM node (bubbling through its
/// ancestors). Returns `Ok(false)` when a listener called `preventDefault()`.
pub(crate) fn dispatch_rust_event(
    context: &mut Context,
    target: JsEventTarget,
    event_type: &str,
    bubbles: bool,
) -> Result<bool, CrateJsError> {
    let target = match target {
        JsEventTarget::Window => TargetKey::Window,
        JsEventTarget::Document => TargetKey::Document,
        JsEventTarget::Node(node) => {
            let handle = with_host(|host| host.handle_for(node))
                .ok_or_else(|| CrateJsError::Internal("no active JavaScript host".to_string()))?
                .map_err(CrateJsError::Dom)?;
            TargetKey::Node(handle)
        }
    };
    let event = build_event(context, event_type, bubbles, true, JsValue::undefined());
    let target_js = match target {
        TargetKey::Window => JsValue::from(context.global_object()),
        TargetKey::Document => global_get(context, "document")
            .map_err(|error| CrateJsError::Internal(error.to_string()))?,
        TargetKey::Node(handle) => crate::runtime::wrap_handle_public(context, handle)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?,
    };
    let Some(event_object) = event.as_object() else {
        return Err(CrateJsError::Internal(
            "event object is not an object".to_string(),
        ));
    };
    let _ = event_object.set(js_string!("target"), target_js.clone(), false, context);
    let path = propagation_path(target, bubbles);
    let _ = with_host(|host| host.dispatch = Some(DispatchState::default()));
    for (phase, key) in &path {
        let stopped = with_host(|host| host.dispatch.map(|dispatch| dispatch.stop_propagation))
            .flatten()
            .unwrap_or(false);
        if stopped {
            break;
        }
        deliver(context, *key, *phase, event_type, &event_object)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
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
    Ok(!prevented)
}

// ---------------------------------------------------------------------------
// matchMedia / viewport / scroll
// ---------------------------------------------------------------------------

fn viewport_info() -> crate::host::ViewportInfo {
    with_host(|host| {
        let mut info = host
            .viewport_provider
            .as_ref()
            .map(|provider| provider())
            .unwrap_or_default();
        info.scroll_x = host.scroll_x;
        info.scroll_y = host.scroll_y;
        info
    })
    .unwrap_or_default()
}

fn window_inner_width(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(f64::from(viewport_info().width)))
}

fn window_inner_height(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(f64::from(viewport_info().height)))
}

fn window_device_pixel_ratio(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info().device_pixel_ratio))
}

fn window_page_x(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info().scroll_x))
}

fn window_page_y(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info().scroll_y))
}

fn match_media(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let query = arg_string(args, 0, context)?;
    let matches = with_host(|host| {
        host.media_provider
            .as_ref()
            .map(|provider| provider(&query))
            .unwrap_or(false)
    })
    .unwrap_or(false);
    let add_listener = accessor_fn(context, noop_listener);
    let remove_listener = accessor_fn(context, noop_listener);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_media_query"), true, Attribute::all())
        .property(
            js_string!("media"),
            js_string!(query.clone()),
            Attribute::all(),
        )
        .property(js_string!("matches"), matches, Attribute::all())
        .property(js_string!("onchange"), JsValue::null(), Attribute::all())
        .function(native(noop_listener), js_string!("addListener"), 1)
        .function(native(noop_listener), js_string!("removeListener"), 1)
        .function(native(noop_listener), js_string!("addEventListener"), 2)
        .function(native(noop_listener), js_string!("removeEventListener"), 2);
    let _ = (add_listener, remove_listener);
    Ok(JsValue::from(init.build()))
}

/// `MediaQueryList` listener registration is accepted but does not fire `change` events
/// until the host re-evaluates the page (documented limitation, like a one-shot render).
fn noop_listener(_this: &JsValue, _args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

fn scroll_coordinates(args: &[JsValue], context: &mut Context) -> JsResult<Option<(f64, f64)>> {
    if let Some(options) = args.first().and_then(|value| value.as_object()) {
        let left = prop(&options, "left", context)
            .ok()
            .and_then(|value| value.as_number())
            .unwrap_or(0.0);
        let top = prop(&options, "top", context)
            .ok()
            .and_then(|value| value.as_number())
            .unwrap_or(0.0);
        return Ok(Some((left, top)));
    }
    let x = args.first().and_then(JsValue::as_number);
    let y = args.get(1).and_then(JsValue::as_number);
    match (x, y) {
        (Some(x), Some(y)) => Ok(Some((x, y))),
        (Some(x), None) => Ok(Some((x, 0.0))),
        _ => Ok(None),
    }
}

fn scroll_to(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if let Some((x, y)) = scroll_coordinates(args, context)? {
        let _ = with_host(|host| {
            host.scroll_x = x.max(0.0);
            host.scroll_y = y.max(0.0);
        });
    }
    Ok(JsValue::undefined())
}

fn scroll_by(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if let Some((x, y)) = scroll_coordinates(args, context)? {
        let _ = with_host(|host| {
            host.scroll_x = (host.scroll_x + x).max(0.0);
            host.scroll_y = (host.scroll_y + y).max(0.0);
        });
    }
    Ok(JsValue::undefined())
}

/// Reports an uncaught script exception: invokes `window.onerror`/`document.onerror`
/// (when installed) with `(message, source, lineno, colno, error)` and records the error
/// in the console capture buffer.
pub(crate) fn report_uncaught(
    context: &mut Context,
    message: &str,
    source: &str,
) -> Result<(), CrateJsError> {
    let window = JsValue::from(context.global_object());
    let document = global_get(context, "document")
        .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    for (target, this) in [(TargetKey::Window, window), (TargetKey::Document, document)] {
        let slot = with_host(|host| {
            host.inline_handlers
                .iter()
                .find(|handler| handler.target == target && handler.event_type == "error")
                .map(|handler| handler.slot)
        })
        .flatten();
        if let Some(slot) = slot {
            let callback = array_get(context, "__megi_inline_handlers", u64::from(slot))
                .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
            if let Some(callback) = callback.as_object() {
                let args = [
                    JsValue::from(js_string!(message)),
                    JsValue::from(js_string!(source)),
                    JsValue::from(0.0),
                    JsValue::from(0.0),
                    JsValue::from(js_string!(message)),
                ];
                let _ = callback.call(&this, &args, context);
            }
        }
    }
    let _ = with_host(|host| {
        host.console.push(crate::host::ConsoleEntry {
            level: "error".to_string(),
            message: format!("Uncaught {message}"),
        });
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// getComputedStyle
// ---------------------------------------------------------------------------

fn get_computed_style(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = args
        .first()
        .and_then(|value| value.as_object())
        .and_then(|object| {
            prop(&object, "__megi_node", context)
                .ok()
                .and_then(|value| value.as_number())
                .map(|value| value as u32)
        })
        .ok_or_else(|| js_error("getComputedStyle expects an element"))?;
    let node_id = node_id_of(node)?;
    let pairs = with_host(|host| {
        host.computed_style_provider
            .as_ref()
            .map(|provider| provider(&host.document, node_id))
    })
    .flatten()
    .unwrap_or_else(|| {
        with_host(|host| {
            host.document
                .try_node(node_id)
                .and_then(|node| node.as_element())
                .and_then(|element| element.get_attribute("style"))
                .map(|style| {
                    css::parser::parse_list_of_declarations(style)
                        .into_iter()
                        .map(|declaration| (declaration.name, declaration.raw_value))
                        .collect()
                })
        })
        .flatten()
        .unwrap_or_default()
    });
    let getter = accessor_fn(context, computed_get_property);
    let setter = accessor_fn(context, computed_set_property);
    let item = accessor_fn(context, computed_item);
    let names = JsArray::new(context);
    for (name, _value) in &pairs {
        let _ = names.push(js_string!(name.clone()), context);
    }
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_computed"), true, Attribute::all())
        .property(
            js_string!("__megi_computed_names"),
            JsValue::from(names),
            Attribute::all(),
        );
    for (name, value) in &pairs {
        init.property(
            js_string!(name.clone()),
            js_string!(value.clone()),
            Attribute::all(),
        );
    }
    init.property(
        js_string!("length"),
        f64::from(pairs.len() as u32),
        Attribute::all(),
    )
    .function(
        NativeFunction::from_fn_ptr(computed_get_property),
        js_string!("getPropertyValue"),
        1,
    )
    .function(
        NativeFunction::from_fn_ptr(computed_set_property),
        js_string!("setProperty"),
        2,
    )
    .function(
        NativeFunction::from_fn_ptr(computed_item),
        js_string!("item"),
        1,
    );
    let css_text = pairs
        .iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ");
    init.property(
        js_string!("cssText"),
        js_string!(css_text),
        Attribute::all(),
    );
    let _ = (getter, setter, item);
    Ok(JsValue::from(init.build()))
}

fn computed_get_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid computed style receiver"))?;
    let value = prop(&object, &name, context).unwrap_or_else(|_| JsValue::from(js_string!("")));
    if value.is_undefined() {
        Ok(JsValue::from(js_string!("")))
    } else {
        Ok(value)
    }
}

fn computed_set_property(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Err(js_error("computed styles are read-only"))
}

fn computed_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let index = args
        .first()
        .and_then(JsValue::as_number)
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid computed style receiver"))?;
    let names = prop(&object, "__megi_computed_names", context)?;
    let names = names
        .as_object()
        .ok_or_else(|| js_error("computed style names missing"))?;
    let name = names
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(name)
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------

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

fn schedule_timer(
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
        args.get(1)
            .and_then(JsValue::as_number)
            .unwrap_or(0.0)
            .max(0.0) as u64
    };
    let id = with_host(|host| {
        if host.timers.len() >= MAX_PENDING_TIMERS {
            return Err("too many pending timers");
        }
        let id = host.next_timer_id;
        host.next_timer_id += 1;
        let now = host.now_ms();
        host.timers.push(TimerEntry {
            id,
            due_ms: now + delay,
            interval_ms: repeat.then_some(delay.max(1)),
            animation,
        });
        Ok(id)
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?
    .map_err(js_error)?;
    array_put(context, "__megi_timer_callbacks", id, callback)?;
    let args_array = JsArray::new(context);
    for argument in args.iter().skip(2) {
        let _ = args_array.push(argument.clone(), context);
    }
    array_put(context, "__megi_timer_args", id, JsValue::from(args_array))?;
    Ok(JsValue::from(id as f64))
}

fn set_timeout(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    schedule_timer(args, context, false, false)
}

fn set_interval(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    schedule_timer(args, context, true, false)
}

/// `requestAnimationFrame(callback)` — scheduled 16 ms ahead on the injected clock.
fn request_animation_frame(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    schedule_timer(args, context, false, true)
}

/// `cancelAnimationFrame(id)`.
fn cancel_animation_frame(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        let id = id.max(0.0) as u64;
        let _ = with_host(|host| host.timers.retain(|timer| timer.id != id));
        release_timer_slots(context, id);
    }
    Ok(JsValue::undefined())
}

fn clear_timer(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if let Some(id) = args.first().and_then(JsValue::as_number) {
        let id = id.max(0.0) as u64;
        let _ = with_host(|host| host.timers.retain(|timer| timer.id != id));
        release_timer_slots(context, id);
    }
    Ok(JsValue::undefined())
}

/// Drops the rooted callback/arguments of a timer so completed/cancelled timers do not keep
/// their closures alive in the `__megi_timer_*` arrays.
fn release_timer_slots(context: &mut Context, id: u64) {
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
        let next = with_host(|host| {
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
        })
        .flatten();
        let Some((index, id, interval, due_ms, animation)) = next else {
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
            let timestamp = with_host(|host| host.now_ms()).unwrap_or(0);
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
    let _ = with_host(|host| host.set_clock(deadline_ms));
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
    let now = with_host(|host| host.advance_clock(delta))
        .ok_or_else(|| CrateJsError::Internal("no active JavaScript host".to_string()))?;
    let executed = run_due_timers(context, now, max_steps)?;
    Ok((now, executed))
}

/// Sets the virtual clock to `deadline_ms` and runs due timers, at most `max_steps` of them.
pub(crate) fn run_until(
    context: &mut Context,
    deadline_ms: u64,
    max_steps: usize,
) -> Result<usize, CrateJsError> {
    let effective = with_host(|host| host.set_clock(deadline_ms))
        .ok_or_else(|| CrateJsError::Internal("no active JavaScript host".to_string()))?;
    run_due_timers(context, effective, max_steps)
}

/// Current scheduler time (virtual or real).
pub(crate) fn now_ms() -> u64 {
    with_host(|host| host.now_ms()).unwrap_or(0)
}
