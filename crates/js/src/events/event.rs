use super::*;

pub(crate) fn target_key(this: &JsValue, context: &mut Context) -> JsResult<TargetKey> {
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

pub(crate) fn node_id_of(context: &Context, handle: u32) -> JsResult<NodeId> {
    host_ref(context)?.node_for_handle(handle).map_err(js_error)
}

// ---------------------------------------------------------------------------
// Event objects
// ---------------------------------------------------------------------------

pub(crate) fn build_event(
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
    let object = init.build();
    set_event_prototype(context, &object, "Event");
    JsValue::from(object)
}

/// Initializes an event object in place, used by the native `Event` constructor when called
/// via `new` (including from the JS subclass prelude, e.g. `KeyboardEvent`).
pub(crate) fn init_event_object(
    context: &mut Context,
    object: &JsObject,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    detail: JsValue,
) {
    let prevent = accessor_fn(context, event_prevent_default);
    let stop = accessor_fn(context, event_stop_propagation);
    let stop_immediate = accessor_fn(context, event_stop_immediate);
    let _ = object.set(js_string!("__megi_event"), true, false, context);
    let _ = object.set(js_string!("type"), js_string!(event_type), false, context);
    let _ = object.set(js_string!("bubbles"), bubbles, false, context);
    let _ = object.set(js_string!("cancelable"), cancelable, false, context);
    let _ = object.set(js_string!("detail"), detail, false, context);
    let _ = object.set(js_string!("target"), JsValue::null(), false, context);
    let _ = object.set(js_string!("currentTarget"), JsValue::null(), false, context);
    let _ = object.set(js_string!("defaultPrevented"), false, false, context);
    let _ = object.set(js_string!("eventPhase"), 0.0, false, context);
    let _ = object.set(
        js_string!("preventDefault"),
        JsValue::from(prevent),
        false,
        context,
    );
    let _ = object.set(
        js_string!("stopPropagation"),
        JsValue::from(stop),
        false,
        context,
    );
    let _ = object.set(
        js_string!("stopImmediatePropagation"),
        JsValue::from(stop_immediate),
        false,
        context,
    );
}

pub(crate) fn event_constructor(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // #1621: the type argument is required; `undefined` must not be coerced to "undefined".
    if args
        .first()
        .is_none_or(|value| value.is_undefined() || value.is_null())
    {
        return Err(js_error("1 argument required, but only 0 present"));
    }
    let event_type = arg_string(args, 0, context)?;
    let options = args.get(1).and_then(|value| value.as_object());
    let bubbles = options
        .as_ref()
        .and_then(|options| {
            prop(options, "bubbles", context)
                .ok()
                .map(|v| v.to_boolean())
        })
        .unwrap_or(false);
    let cancelable = options
        .as_ref()
        .and_then(|options| {
            prop(options, "cancelable", context)
                .ok()
                .map(|v| v.to_boolean())
        })
        .unwrap_or(false);
    if let Some(object) = this.as_object() {
        init_event_object(
            context,
            &object,
            &event_type,
            bubbles,
            cancelable,
            JsValue::undefined(),
        );
        return Ok(JsValue::from(object));
    }
    Ok(build_event(
        context,
        &event_type,
        bubbles,
        cancelable,
        JsValue::undefined(),
    ))
}

pub(crate) fn custom_event_constructor(
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
                .map(|v| v.to_boolean())
        })
        .unwrap_or(false);
    // #1619: CustomEventInit.cancelable defaults to false (DOM spec).
    let cancelable = options
        .as_ref()
        .and_then(|options| {
            prop(options, "cancelable", context)
                .ok()
                .map(|v| v.to_boolean())
        })
        .unwrap_or(false);
    let event = build_event(context, &event_type, bubbles, cancelable, detail);
    if let Some(object) = event.as_object() {
        set_event_prototype(context, &object, "CustomEvent");
    }
    Ok(event)
}

pub(crate) fn build_message_event(
    context: &mut Context,
    event_type: &str,
    data: JsValue,
    origin: &str,
    source: JsValue,
) -> JsValue {
    let ev_val = build_event(context, event_type, false, false, JsValue::undefined());
    if let Some(obj) = ev_val.as_object() {
        let _ = obj.set(js_string!("data"), data, false, context);
        let _ = obj.set(js_string!("origin"), js_string!(origin), false, context);
        let _ = obj.set(js_string!("source"), source, false, context);
        let _ = obj.set(js_string!("lastEventId"), js_string!(""), false, context);
        set_event_prototype(context, &obj, "MessageEvent");
    }
    ev_val
}

pub(crate) fn message_event_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let event_type = arg_string(args, 0, context)?;
    let options = args.get(1).and_then(|value| value.as_object());
    let data = options
        .as_ref()
        .map(|o| prop(o, "data", context).unwrap_or_else(|_| JsValue::null()))
        .unwrap_or_else(JsValue::null);
    let origin = options
        .as_ref()
        .and_then(|o| prop(o, "origin", context).ok())
        .and_then(|v| v.to_string(context).ok())
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default();
    let source = options
        .as_ref()
        .map(|o| prop(o, "source", context).unwrap_or_else(|_| JsValue::null()))
        .unwrap_or_else(JsValue::null);
    Ok(build_message_event(
        context,
        &event_type,
        data,
        &origin,
        source,
    ))
}

/// Dispatches a `message` event (`MessageEvent`) directly on `window`.
pub(crate) fn dispatch_message_event(
    target: &JsValue,
    data: JsValue,
    origin: &str,
    source: JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let event = build_message_event(context, "message", data, origin, source);
    let result = dispatch_event(target, &[event], context)?;
    Ok(result.to_boolean())
}

pub(crate) fn check_event(this: &JsValue, context: &mut Context) -> JsResult<JsObject> {
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

pub(crate) fn event_prevent_default(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    if let Some(dispatch) = &mut host_cell(context)?.dispatch
        && !dispatch.passive
    {
        // `{ passive: true }` listeners cannot cancel the default action.
        dispatch.prevent_default = true;
    }
    Ok(JsValue::undefined())
}

pub(crate) fn event_stop_propagation(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    if let Some(dispatch) = &mut host_cell(context)?.dispatch {
        dispatch.stop_propagation = true;
    }
    Ok(JsValue::undefined())
}

pub(crate) fn event_stop_immediate(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    check_event(this, context)?;
    if let Some(dispatch) = &mut host_cell(context)?.dispatch {
        dispatch.stop_immediate = true;
    }
    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// EventTarget methods
// ---------------------------------------------------------------------------
