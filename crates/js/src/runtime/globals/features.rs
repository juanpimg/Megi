use super::*;

/// `new Image()` creates a detached HTML `<img>` element (DOM HTMLImageElement).
fn image_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node =
        host_cell(context)?
            .document
            .create_element(Namespace::Html, "img".to_owned(), Vec::new());
    let wrapper = wrap_node(context, node)?;
    if let Some(src) = args.first().and_then(|value| value.as_string())
        && let Some(object) = wrapper.as_object()
    {
        let _ = object.set(js_string!("src"), js_string!(src), false, context);
    }
    Ok(wrapper)
}

/// Installs History/location, Web Storage and MutationObserver (round 24).
/// `new Element()` is illegal per Web IDL, but the constructor must exist for
/// `instanceof Element` and `Element.prototype` (#1877).
fn element_illegal_constructor(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Err(boa_engine::JsNativeError::typ()
        .with_message("Illegal constructor")
        .into())
}

pub(crate) fn install_browser_features(context: &mut Context) -> Result<(), JsError> {
    for name in [
        "__megi_history_state",
        "__megi_observer_callbacks",
        "__megi_observer_objects",
    ] {
        let array = JsArray::from_iter([], context);
        context
            .global_object()
            .set(js_string!(name), JsValue::from(array), false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    let history = build_history(context)?;
    let location = build_location(context)?;
    let local_storage = build_storage(context, StorageKind::Local)?;
    let session_storage = build_storage(context, StorageKind::Session)?;
    for (name, value) in [
        ("history", JsValue::from(history)),
        ("location", JsValue::from(location)),
        ("localStorage", JsValue::from(local_storage)),
        ("sessionStorage", JsValue::from(session_storage)),
    ] {
        context
            .global_object()
            .set(js_string!(name), value, false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    // F7-A11Y (partial): deterministic accessibility snapshot for embedders/tests.
    context
        .global_object()
        .set(
            js_string!("__megiAccessibilityTree"),
            JsValue::from(accessor_fn(context, accessibility_tree_snapshot)),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    // #1882: `self` aliases the global object and feature-detected DOM constructors exist.
    context
        .global_object()
        .set(
            js_string!("self"),
            JsValue::from(context.global_object()),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    for name in ["EventTarget", "HTMLLinkElement"] {
        let ctor = FunctionObjectBuilder::new(context.realm(), native(element_illegal_constructor))
            .name(js_string!(name))
            .length(0)
            .constructor(true)
            .build();
        context
            .global_object()
            .set(js_string!(name), JsValue::from(ctor), false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    let image_ctor = FunctionObjectBuilder::new(context.realm(), native(image_constructor))
        .name(js_string!("Image"))
        .length(0)
        .constructor(true)
        .build();
    context
        .global_object()
        .set(
            js_string!("Image"),
            JsValue::from(image_ctor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    // #1877: Web IDL/DOM exposes a global `Element` constructor with a prototype.
    let element_constructor =
        FunctionObjectBuilder::new(context.realm(), native(element_illegal_constructor))
            .name(js_string!("Element"))
            .length(0)
            .constructor(true)
            .build();
    context
        .global_object()
        .set(
            js_string!("Element"),
            JsValue::from(element_constructor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    let constructor =
        FunctionObjectBuilder::new(context.realm(), native(mutation_observer_constructor))
            .name(js_string!("MutationObserver"))
            .length(1)
            .constructor(true)
            .build();
    let worker_constructor =
        FunctionObjectBuilder::new(context.realm(), native(worker_constructor))
            .name(js_string!("Worker"))
            .length(1)
            .constructor(true)
            .build();
    context
        .global_object()
        .set(
            js_string!("Worker"),
            JsValue::from(worker_constructor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .global_object()
        .set(
            js_string!("MutationObserver"),
            JsValue::from(constructor),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    let text_encoder = build_text_encoder(context);
    let text_decoder = build_text_decoder(context);
    context
        .global_object()
        .set(
            js_string!("TextEncoder"),
            JsValue::from(text_encoder),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .global_object()
        .set(
            js_string!("TextDecoder"),
            JsValue::from(text_decoder),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    let navigator = build_navigator(context);
    let performance = build_performance(context);
    let crypto = build_crypto(context);
    for (name, value) in [
        ("navigator", JsValue::from(navigator)),
        ("performance", JsValue::from(performance)),
        ("crypto", JsValue::from(crypto)),
    ] {
        context
            .global_object()
            .set(js_string!(name), value, false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    install_frame_globals(context)?;
    Ok(())
}

pub(crate) fn build_history(context: &mut Context) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(history_push_state), js_string!("pushState"), 3);
    init.function(native(history_replace_state), js_string!("replaceState"), 3);
    init.function(native(history_back), js_string!("back"), 0);
    init.function(native(history_forward), js_string!("forward"), 0);
    init.function(native(history_go), js_string!("go"), 1);
    let history = init.build();
    define_accessor(context, &history, "length", history_length_get, None)?;
    define_accessor(context, &history, "state", history_state_get, None)?;
    Ok(history)
}

pub(crate) fn build_location(context: &mut Context) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(location_assign), js_string!("assign"), 1);
    init.function(native(location_replace), js_string!("replace"), 1);
    init.function(native(location_reload), js_string!("reload"), 0);
    let location = init.build();
    for (name, getter, setter) in [
        (
            "href",
            location_href_get as NativeFn,
            Some(location_href_set as NativeFn),
        ),
        ("pathname", location_pathname_get as NativeFn, None),
        ("search", location_search_get as NativeFn, None),
        ("hash", location_hash_get as NativeFn, None),
        ("origin", location_origin_get as NativeFn, None),
        ("protocol", location_protocol_get as NativeFn, None),
        ("host", location_host_get as NativeFn, None),
    ] {
        let get = accessor_fn(context, getter);
        let descriptor = match setter {
            Some(setter) => {
                let set = accessor_fn(context, setter);
                PropertyDescriptor::builder()
                    .get(JsValue::from(get))
                    .set(JsValue::from(set))
                    .enumerable(true)
                    .configurable(true)
                    .build()
            }
            None => PropertyDescriptor::builder()
                .get(JsValue::from(get))
                .enumerable(true)
                .configurable(true)
                .build(),
        };
        location
            .define_property_or_throw(js_string!(name), descriptor, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    Ok(location)
}

pub(crate) fn define_accessor(
    context: &mut Context,
    object: &JsObject,
    name: &str,
    getter: NativeFn,
    setter: Option<NativeFn>,
) -> Result<(), JsError> {
    let get = accessor_fn(context, getter);
    let descriptor = match setter {
        Some(setter) => {
            let set = accessor_fn(context, setter);
            PropertyDescriptor::builder()
                .get(JsValue::from(get))
                .set(JsValue::from(set))
                .enumerable(true)
                .configurable(true)
                .build()
        }
        None => PropertyDescriptor::builder()
            .get(JsValue::from(get))
            .enumerable(true)
            .configurable(true)
            .build(),
    };
    object
        .define_property_or_throw(js_string!(name), descriptor, context)
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}

pub(crate) fn build_storage(context: &mut Context, kind: StorageKind) -> Result<JsObject, JsError> {
    let mut init = ObjectInitializer::new(context);
    init.function(native(storage_get_item), js_string!("getItem"), 1);
    init.function(native(storage_set_item), js_string!("setItem"), 2);
    init.function(native(storage_remove_item), js_string!("removeItem"), 1);
    init.function(native(storage_clear), js_string!("clear"), 0);
    init.function(native(storage_key), js_string!("key"), 1);
    let storage = init.build();
    storage
        .set(
            js_string!("__megi_storage_kind"),
            JsValue::from(kind == StorageKind::Session),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    define_accessor(context, &storage, "length", storage_length_get, None)?;
    Ok(storage)
}

pub(crate) fn build_console(context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    let methods: [(&str, NativeFn); 8] = [
        ("log", console_log),
        ("info", console_info),
        ("warn", console_warn),
        ("error", console_error),
        ("debug", console_debug),
        ("assert", console_assert),
        ("table", console_table),
        ("dir", console_debug),
    ];
    for (name, function) in methods {
        init.function(native(function), js_string!(name), 0);
    }
    init.build()
}

pub(crate) fn console_log(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "log")
}
pub(crate) fn console_info(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "info")
}
pub(crate) fn console_warn(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "warn")
}
pub(crate) fn console_error(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "error")
}
pub(crate) fn console_debug(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "debug")
}
pub(crate) fn console_table(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    record_console(this, args, context, "table")
}

/// `console.assert(condition, ...)` — logs only when the condition is falsy.
pub(crate) fn console_assert(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let truthy = args
        .first()
        .map(|value| value.to_boolean())
        .unwrap_or(false);
    if truthy {
        return Ok(JsValue::undefined());
    }
    let rest = args.get(1..).unwrap_or_default();
    let mut parts = vec!["Assertion failed".to_string()];
    for argument in rest {
        parts.push(match argument.to_string(context) {
            Ok(text) => text.to_std_string_escaped(),
            Err(_) => "<unprintable>".to_string(),
        });
    }
    let message = parts.join(" ");
    let _ = with_context_host(context, |host| {
        host.push_console(ConsoleEntry {
            level: "assert".to_string(),
            message,
        })
    });
    Ok(JsValue::undefined())
}

/// `console.*` — space-joined argument stringification into the capture buffer.
pub(crate) fn record_console(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    level: &str,
) -> JsResult<JsValue> {
    let mut parts = Vec::with_capacity(args.len());
    for argument in args {
        parts.push(match argument.to_string(context) {
            Ok(text) => text.to_std_string_escaped(),
            Err(_) => "<unprintable>".to_string(),
        });
    }
    let message = parts.join(" ");
    let level = level.to_string();
    let _ = with_context_host(context, |host| {
        host.push_console(ConsoleEntry { level, message })
    });
    Ok(JsValue::undefined())
}

/// Builds the `document` host object.
pub(crate) fn build_document_object(context: &mut Context) -> JsObject {
    let pick = |context: &mut Context,
                get: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>| {
        accessor_fn(context, get)
    };
    let document_element = pick(context, document_element);
    let document_current_script = pick(context, document_current_script);
    let document_body = pick(context, document_body);
    let document_head = pick(context, document_head);
    let title_get = pick(context, document_title_get);
    let title_set = pick(context, document_title_set);
    let url = pick(context, document_url);
    let ready_state = pick(context, document_ready_state);
    let inline_accessors: Vec<(
        &'static str,
        boa_engine::object::builtins::JsFunction,
        boa_engine::object::builtins::JsFunction,
    )> = crate::events::inline_node_accessors()
        .into_iter()
        .map(|(name, get, set)| (name, accessor_fn(context, get), accessor_fn(context, set)))
        .collect();
    let active_element = pick(context, document_active_element);
    let cookie_get = pick(context, document_cookie_get);
    let cookie_set = pick(context, document_cookie_set);
    let mut init = ObjectInitializer::new(context);
    // Methods.
    for (name, length, function) in [
        (
            "write",
            1usize,
            document_write as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("writeln", 1, document_writeln),
        ("getElementById", 1, document_get_element_by_id),
        ("getElementsByTagName", 1, document_get_elements_by_tag_name),
        (
            "getElementsByClassName",
            1,
            document_get_elements_by_class_name,
        ),
        ("querySelector", 1, document_query_selector),
        ("querySelectorAll", 1, document_query_selector_all),
        ("createElement", 1, document_create_element),
        ("createTextNode", 1, document_create_text_node),
        (
            "createDocumentFragment",
            0,
            document_create_document_fragment,
        ),
        ("normalize", 0, document_normalize),
        (
            "compareDocumentPosition",
            2,
            document_compare_document_position,
        ),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    // Properties.
    init.accessor(
        js_string!("documentElement"),
        Some(document_element),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("currentScript"),
        Some(document_current_script),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("body"),
        Some(document_body),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("head"),
        Some(document_head),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("title"),
        Some(title_get),
        Some(title_set),
        Attribute::all(),
    );
    init.accessor(
        js_string!("activeElement"),
        Some(active_element),
        None,
        Attribute::all(),
    );
    init.accessor(
        js_string!("cookie"),
        Some(cookie_get),
        Some(cookie_set),
        Attribute::all(),
    );
    init.accessor(js_string!("URL"), Some(url), None, Attribute::all());
    init.accessor(
        js_string!("readyState"),
        Some(ready_state),
        None,
        Attribute::all(),
    );
    init.property(js_string!("__megi_document"), true, Attribute::all());
    for (name, length, function) in crate::events::node_methods() {
        init.function(native(function), js_string!(name), length);
    }
    for (name, get, set) in inline_accessors {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    init.function(native(document_close), js_string!("close"), 0);
    init.build()
}
