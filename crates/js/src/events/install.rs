use super::*;

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
        let array = JsArray::from_iter([], context);
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
        ("MessageEvent", 2, message_event_constructor),
    ] {
        // #1740: a real constructor (with a `prototype` object) is required by ECMA-262
        // §7.3.7 `instanceof`; `register_global_callable` has no prototype property.
        let constructor = FunctionObjectBuilder::new(context.realm(), native(function))
            .name(js_string!(name))
            .length(length)
            .constructor(true)
            .build();
        // Boa's builder does not guarantee a `prototype` property in every configuration;
        // `instanceof` requires it to be an object (ECMA-262 §7.3.7).
        if constructor
            .get(js_string!("prototype"), context)
            .ok()
            .and_then(|value| value.as_object())
            .is_none()
        {
            let proto = ObjectInitializer::new(context).build();
            let _ = constructor.set(
                js_string!("prototype"),
                JsValue::from(proto),
                false,
                context,
            );
        }
        context
            .global_object()
            .set(js_string!(name), JsValue::from(constructor), false, context)
            .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    }
    // Event subclasses inherit from Event.prototype, so `instanceof Event` is true for them.
    let event_proto = context
        .global_object()
        .get(js_string!("Event"), context)
        .ok()
        .and_then(|value| value.as_object())
        .and_then(|ctor| ctor.get(js_string!("prototype"), context).ok())
        .and_then(|value| value.as_object());
    if let Some(event_proto) = event_proto {
        for name in ["CustomEvent", "MessageEvent"] {
            if let Some(proto) = context
                .global_object()
                .get(js_string!(name), context)
                .ok()
                .and_then(|value| value.as_object())
                .and_then(|ctor| ctor.get(js_string!("prototype"), context).ok())
                .and_then(|value| value.as_object())
            {
                let _ = proto.set_prototype(Some(event_proto.clone()));
            }
        }
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
        ("scrollX", window_page_x),
        ("scrollY", window_page_y),
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

/// Bare calls like `addEventListener("error", fn)` use `globalThis` as the receiver in
/// strict mode, which is not an EventTarget wrapper. Bind these three window methods to the
/// realm global object (#1894).
fn global_add_listener(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let window = JsValue::from(context.global_object());
    add_listener(&window, args, context)
}

fn global_remove_listener(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let window = JsValue::from(context.global_object());
    remove_listener(&window, args, context)
}

fn global_dispatch_event(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let window = JsValue::from(context.global_object());
    dispatch_event(&window, args, context)
}

pub(crate) fn window_methods() -> [(&'static str, usize, NativeFn); 7] {
    [
        ("addEventListener", 2, global_add_listener),
        ("removeEventListener", 2, global_remove_listener),
        ("dispatchEvent", 1, global_dispatch_event),
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
