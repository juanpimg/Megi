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
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // WHATWG Custom Elements §4.4.3: during a registered constructor's `super()` call,
    // the HTMLElement constructor creates the element with the registry's local name.
    let mut tag =
        with_context_host(context, |host| host.custom_element_construction.take()).flatten();
    // Boa passes `new.target` (the constructor function) as `this` for `new`.
    // WHATWG Custom Elements §4.4.3: compare its `prototype` with the registry.
    let new_target_prototype = this
        .as_object()
        .filter(|object| object.is_callable())
        .and_then(|object| object.get(js_string!("prototype"), context).ok())
        .and_then(|value| value.as_object())
        .or_else(|| this.as_object().and_then(|object| object.prototype()));
    if tag.is_none() && new_target_prototype.is_some() {
        // Direct `new X()` in a custom element constructor's `super()` carries the
        // registered constructor's prototype as the fresh `this` prototype (WHATWG
        // Custom Elements §4.4.3). Match it against the registry to infer the local name.
        let registered = with_context_host(context, |host| {
            host.custom_elements
                .iter()
                .map(|(name, handle)| (name.clone(), *handle))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
        for (name, handle) in registered {
            if let Ok(constructor) = context
                .global_object()
                .get(js_string!(format!("__megi_ce_{handle}")), context)
                && let Some(constructor) = constructor.as_callable()
                && let Ok(prototype) = constructor.get(js_string!("prototype"), context)
                && prototype.as_object() == new_target_prototype
            {
                tag = Some(name);
                break;
            }
        }
    }
    // WHATWG Custom Elements §4.2: an upgrade reaction reuses the existing wrapper;
    // `super()` returns that object to the derived constructor instead of a new node.
    if let Some(target) =
        with_context_host(context, |host| host.custom_element_upgrade_target.take()).flatten()
    {
        let wrapper = wrap_node(context, target)?;
        if let (Some(object), Some(prototype)) =
            (wrapper.as_object(), new_target_prototype.as_ref())
        {
            object.set_prototype(Some(prototype.clone()));
        }
        return Ok(wrapper);
    }
    if let Some(tag) = tag {
        let node = with_context_host(context, |host| {
            host.document
                .create_element(Namespace::Html, tag, Vec::new())
        });
        if let Some(node) = node {
            let wrapper = wrap_node(context, node)?;
            // WHATWG Custom Elements §4.4.3: `new X()` must inherit from X.prototype,
            // not from HTMLElement.prototype (so `instanceof X` holds).
            if let (Some(object), Some(prototype)) =
                (wrapper.as_object(), new_target_prototype.as_ref())
            {
                object.set_prototype(Some(prototype.clone()));
            }
            return Ok(wrapper);
        }
    }
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
    // #1877/#1882: a global `Element` constructor with a real `.prototype` object.
    // `FunctionObjectBuilder::constructor(true)` does NOT create `prototype` in Boa 0.22;
    // `ConstructorBuilder` does, so `instanceof Element` works.
    let mut element_builder =
        boa_engine::object::ConstructorBuilder::new(context, native(element_illegal_constructor));
    element_builder.name("Element").length(0).constructor(true);
    let element_constructor = element_builder.build();
    context
        .global_object()
        .set(
            js_string!("Element"),
            JsValue::from(element_constructor.constructor()),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    // WHATWG DOM §4.9: the prelude installs the validating `Element.prototype.attachShadow`
    // on top of this native entry point (it needs to throw real DOMExceptions).
    let _ = context.register_global_builtin_callable(
        js_string!("__megi_slot_assigned_nodes_native"),
        0,
        native(crate::runtime::slot_assigned_nodes),
    );
    let _ = context.register_global_builtin_callable(
        js_string!("__megi_assigned_slot_native"),
        0,
        native(crate::runtime::node_assigned_slot_get),
    );
    let _ = context.register_global_builtin_callable(
        js_string!("__megi_attach_shadow_native"),
        1,
        native(crate::runtime::node_attach_shadow),
    );
    let _ = context.register_global_builtin_callable(
        js_string!("__megi_shadow_root_native"),
        0,
        native(crate::runtime::node_shadow_root_get),
    );
    // #1915: `HTMLElement` is a subclass of Element; element wrappers inherit from
    // its prototype so both `instanceof Element` and `instanceof HTMLElement` hold.
    let element_proto = element_constructor
        .constructor()
        .get(js_string!("prototype"), context)?;
    // #2245: Element.prototype must expose the DOM query methods so libraries
    // that call `Element.prototype.querySelectorAll.call(el, sel)` work.
    if let Some(proto) = element_proto.as_object() {
        for (name, function, length) in [
            (
                "querySelector",
                crate::runtime::node_query_selector
                    as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
                1usize,
            ),
            (
                "querySelectorAll",
                crate::runtime::node_query_selector_all,
                1,
            ),
        ] {
            let _ = proto.set(
                js_string!(name),
                JsValue::from(
                    boa_engine::object::FunctionObjectBuilder::new(
                        context.realm(),
                        native(function),
                    )
                    .name(name)
                    .length(length)
                    .build(),
                ),
                false,
                context,
            );
        }
    }
    let mut html_builder =
        boa_engine::object::ConstructorBuilder::new(context, native(element_illegal_constructor));
    html_builder.name("HTMLElement").length(0).constructor(true);
    let html_constructor = html_builder.build();
    if let (Some(html_proto), Some(element_proto)) = (
        html_constructor
            .constructor()
            .get(js_string!("prototype"), context)?
            .as_object(),
        element_proto.as_object(),
    ) {
        let _ = html_proto.set_prototype(Some(element_proto.clone()));
    }
    context
        .global_object()
        .set(
            js_string!("HTMLElement"),
            JsValue::from(html_constructor.constructor()),
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
    // #1935: `String(location)` / `new URL('.', location)` need a string representation.
    init.function(native(location_href_get), js_string!("toString"), 0);
    init.function(native(location_href_get), js_string!("valueOf"), 0);
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
        ("hostname", location_hostname_get as NativeFn, None),
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

/// DOM `DOMImplementation.createHTMLDocument(title)` — returns a minimal
/// document whose `body` is a real element wrapper (with `innerHTML`/`childNodes`),
/// enough for library feature detection such as jQuery 3.5.1 (#1880).
fn document_impl_create_html_document(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let body = with_context_host(context, |host| {
        host.document
            .create_element(Namespace::Html, "body".to_owned(), Vec::new())
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    let body = wrap_node(context, body)?;
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("body"), body, Attribute::all());
    Ok(JsValue::from(init.build()))
}

/// Builds the `document.implementation` host object (DOMImplementation).
fn build_document_implementation(context: &mut Context) -> JsObject {
    let create = accessor_fn(context, document_impl_create_html_document);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("createHTMLDocument"), create, Attribute::all());
    init.build()
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
    // DOM: `document` is a Node, so it exposes `childNodes` (#1880). jQuery's
    // Sizzle initialization calls `Array.prototype.slice.call(document.childNodes)`;
    // without this accessor the value is undefined and the whole library aborts.
    let child_nodes = pick(context, node_child_nodes);
    // WHATWG DOM: `document.nodeType` is 9 (DOCUMENT_NODE). jQuery's setDocument()
    // gates initialization on `9 === a.nodeType`, so omitting it aborts the library (#1880).
    let node_type = pick(context, node_node_type);
    // #2245: document.location (alias of window.location) is read by paywall
    // bundles as `document.location.protocol`.
    let document_location = build_location(context).ok();
    let implementation = build_document_implementation(context);
    let cookie_get = pick(context, document_cookie_get);
    let cookie_set = pick(context, document_cookie_set);
    // WHATWG DOM: `document` is itself a Node. Expose the root's handle so
    // MutationObserver.observe(document) and other node APIs accept it (#1905).
    let root_handle = with_context_host(context, |host| {
        let root = host.document.root();
        host.handle_for(root).ok()
    })
    .flatten();

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
        ("createElementNS", 2, document_create_element_ns),
        ("createTextNode", 1, document_create_text_node),
        ("createComment", 1, document_create_comment),
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
        ("toString", 0, document_url),
        ("valueOf", 0, document_url),
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
        js_string!("childNodes"),
        Some(child_nodes),
        None,
        Attribute::all(),
    );
    if let Some(document_location) = document_location {
        init.property(
            js_string!("location"),
            JsValue::from(document_location),
            Attribute::all(),
        );
    }
    init.accessor(
        js_string!("nodeType"),
        Some(node_type),
        None,
        Attribute::all(),
    );
    init.property(
        js_string!("implementation"),
        JsValue::from(implementation),
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
    if let Some(handle) = root_handle {
        init.property(
            js_string!("__megi_node"),
            JsValue::from(f64::from(handle)),
            Attribute::all(),
        );
    }
    for (name, length, function) in crate::events::node_methods() {
        init.function(native(function), js_string!(name), length);
    }
    for (name, get, set) in inline_accessors {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    init.function(native(document_close), js_string!("close"), 0);
    init.build()
}
