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
