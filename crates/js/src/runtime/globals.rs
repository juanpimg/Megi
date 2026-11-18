use super::*;

// ---------------------------------------------------------------------------
// Global installation
// ---------------------------------------------------------------------------

pub(crate) fn native(
    f: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> NativeFunction {
    NativeFunction::from_fn_ptr(f)
}

pub(crate) fn accessor_fn(
    context: &mut Context,
    f: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> boa_engine::object::builtins::JsFunction {
    FunctionObjectBuilder::new(context.realm(), native(f)).build()
}

/// Borrows the host attached to `context` for the duration of `f`.
///
/// This is the single access path for native bindings. It keeps the `Option<R>`
/// shape so each call site's fallback (`unwrap_or`, `ok_or_else`, …) is concise;
/// the underlying [`host_cell`] borrow is a `RefMut` dropped as soon as `f`
/// returns, so no borrow survives a call back into JavaScript. Re-entry is a
/// typed Boa error (mapped to `None` here) instead of a panic. A mapped `None`
/// means a re-entrant call was rejected; bindings keep their pre-existing
/// fallback semantics because `Option` already encoded "host unavailable"
/// before this migration. A future task should split the two cases into a
/// typed error for mutating bindings.
pub(crate) fn with_context_host<R>(
    context: &mut Context,
    f: impl FnOnce(&mut Host) -> R,
) -> Option<R> {
    host_cell(context).ok().map(|mut host| f(&mut host))
}

/// Clones a provider out of the context's immutable [`crate::providers::HostProviders`]
/// slot, falling back to the legacy host mirror when no slot was installed.
///
/// The clone is an `Arc`, so a binding can drop its host borrow *before* invoking the
/// callback. A provider is allowed to re-enter JavaScript; calling it while a `RefMut`
/// of the host is alive would otherwise panic with `already borrowed`.
pub(crate) fn ctx_or_mirror<T>(
    context: &mut Context,
    from_slot: impl FnOnce(&crate::providers::HostProviders) -> Option<T>,
    from_mirror: impl FnOnce(&mut Host) -> Option<T>,
) -> Option<T> {
    {
        let slot = crate::providers::providers(context);
        if let Some(providers) = slot
            && let Some(value) = from_slot(providers)
        {
            return Some(value);
        }
    }
    with_context_host(context, from_mirror).flatten()
}

/// Installs only non-DOM worker globals (`console` and timers).
pub(crate) fn install_bare_worker_globals(context: &mut Context) -> Result<(), JsError> {
    let global = context.global_object();
    let console = build_console(context);
    global
        .set(
            js_string!("console"),
            JsValue::from(console),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    for (name, length, function) in crate::events::timer_globals() {
        let builtin = FunctionObjectBuilder::new(context.realm(), native(function))
            .name(js_string!(name))
            .length(length)
            .build();
        global
            .set(js_string!(name), JsValue::from(builtin), false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    Ok(())
}

/// Installs `window`, `console`, `document`, `setTimeout`/`setInterval` and helpers.
pub(crate) fn install_globals(context: &mut Context) -> Result<(), JsError> {
    let global = context.global_object();
    let window_value = JsValue::from(global.clone());

    let console = build_console(context);
    let document = build_document_object(context);
    let entries: [(&str, JsValue); 3] = [
        ("window", window_value),
        ("console", JsValue::from(console)),
        ("document", JsValue::from(document)),
    ];
    for (name, value) in entries {
        global
            .set(js_string!(name), value, false, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    crate::events::install(context)?;
    install_browser_features(context)?;
    install_gap_apis(context)?;
    crate::xhr::install_xhr(context)?;
    crate::websocket::install(context).map_err(|error| JsError::Internal(error.to_string()))?;
    let registry = ObjectInitializer::new(context)
        .function(native(custom_elements_define), js_string!("define"), 2)
        .function(native(custom_elements_get), js_string!("get"), 1)
        .build();
    global
        .set(
            js_string!("customElements"),
            JsValue::from(registry),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}

/// Builds the `navigator` host object.
pub(crate) fn build_navigator(context: &mut Context) -> JsObject {
    let languages = JsArray::from_iter([JsValue::from(js_string!("en-US"))], context);
    let plugins = JsArray::from_iter([], context);
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("userAgent"),
        js_string!("Megi/1.0"),
        Attribute::all(),
    );
    init.property(
        js_string!("language"),
        js_string!("en-US"),
        Attribute::all(),
    );
    init.property(
        js_string!("languages"),
        JsValue::from(languages),
        Attribute::all(),
    );
    init.property(js_string!("onLine"), JsValue::from(true), Attribute::all());
    init.property(
        js_string!("hardwareConcurrency"),
        JsValue::from(1),
        Attribute::all(),
    );
    init.property(
        js_string!("platform"),
        js_string!("Linux x86_64"),
        Attribute::all(),
    );
    init.property(
        js_string!("plugins"),
        JsValue::from(plugins),
        Attribute::all(),
    );
    init.function(
        native(|_this, _args, _context| Ok(JsValue::from(true))),
        js_string!("sendBeacon"),
        0,
    );
    init.build()
}

/// Builds the `performance` host object.
pub(crate) fn build_performance(context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    init.function(
        native(|_this, _args, _context| {
            let now = with_context_host(_context, |host| host.now_ms()).unwrap_or(0);
            Ok(JsValue::from(now as f64))
        }),
        js_string!("now"),
        0,
    );
    init.property(
        js_string!("timeOrigin"),
        JsValue::from(0.0_f64),
        Attribute::all(),
    );
    for name in [
        "mark",
        "measure",
        "getEntries",
        "getEntriesByName",
        "getEntriesByType",
        "clearMarks",
        "clearMeasures",
    ] {
        init.function(
            native(|_this, _args, _context| Ok(JsValue::undefined())),
            js_string!(name),
            0,
        );
    }
    init.build()
}

/// Builds the `crypto` host object.
pub(crate) fn build_crypto(context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    init.function(
        native(|_this, args, context| {
            let array = args.first().cloned().unwrap_or(JsValue::undefined());
            let obj = array
                .as_object()
                .ok_or_else(|| js_error("getRandomValues requires a typed array"))?;
            // Fill with pseudo-random bytes using a simple xorshift seeded by the clock.
            let mut seed = with_context_host(context, |host| host.now_ms()).unwrap_or(12345);
            let len = obj
                .get(js_string!("length"), context)
                .ok()
                .and_then(|v| v.to_number(context).ok())
                .unwrap_or(0.0) as usize;
            for i in 0..len {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let byte = (seed & 0xff) as u8;
                let _ = obj.set(
                    js_string!(i.to_string()),
                    JsValue::from(byte as i32),
                    false,
                    context,
                );
            }
            Ok(array)
        }),
        js_string!("getRandomValues"),
        1,
    );
    init.function(
        native(|_this, _args, _context| {
            // Generate a v4 UUID from the same xorshift source.
            let mut seed = with_context_host(_context, |host| host.now_ms()).unwrap_or(12345);
            let mut bytes = [0u8; 16];
            for b in bytes.iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *b = (seed & 0xff) as u8;
            }
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
            let uuid = format!(
                "{}-{}-{}-{}-{}",
                &hex[0..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..32]
            );
            Ok(JsValue::from(js_string!(uuid)))
        }),
        js_string!("randomUUID"),
        0,
    );
    init.build()
}

/// Builds the `TextEncoder` constructor.
pub(crate) fn build_text_encoder(
    context: &mut Context,
) -> boa_engine::object::builtins::JsFunction {
    let constructor = FunctionObjectBuilder::new(context.realm(), native(text_encoder_constructor))
        .name(js_string!("TextEncoder"))
        .length(0)
        .constructor(true)
        .build();
    let proto = constructor
        .get(js_string!("prototype"), context)
        .unwrap_or(JsValue::undefined())
        .as_object()
        .unwrap_or_else(|| ObjectInitializer::new(context).build());
    let encode_fn = FunctionObjectBuilder::new(context.realm(), native(text_encoder_encode))
        .name(js_string!("encode"))
        .length(1)
        .build();
    let _ = proto.set(
        js_string!("encode"),
        JsValue::from(encode_fn),
        false,
        context,
    );
    let encoding_get = accessor_fn(context, text_encoder_encoding_get);
    let _ = proto.define_property_or_throw(
        js_string!("encoding"),
        PropertyDescriptor::builder()
            .get(JsValue::from(encoding_get))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );
    constructor
}

pub(crate) fn text_encoder_constructor(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    Ok(JsValue::from(init.build()))
}

pub(crate) fn text_encoder_encoding_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!("utf-8")))
}

pub(crate) fn text_encoder_encode(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let input = args
        .first()
        .map(|v| {
            v.to_string(context)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let block = AlignedVec::from_iter(0, input.as_bytes().iter().copied());
    let buffer =
        JsArrayBuffer::from_byte_block(block, context).map_err(|e| js_error(e.to_string()))?;
    let arr =
        JsUint8Array::from_array_buffer(buffer, context).map_err(|e| js_error(e.to_string()))?;
    Ok(JsValue::from(arr))
}

/// Builds the `TextDecoder` constructor.
pub(crate) fn build_text_decoder(
    context: &mut Context,
) -> boa_engine::object::builtins::JsFunction {
    let constructor = FunctionObjectBuilder::new(context.realm(), native(text_decoder_constructor))
        .name(js_string!("TextDecoder"))
        .length(0)
        .constructor(true)
        .build();
    let proto = constructor
        .get(js_string!("prototype"), context)
        .unwrap_or(JsValue::undefined())
        .as_object()
        .unwrap_or_else(|| ObjectInitializer::new(context).build());
    let decode_fn = FunctionObjectBuilder::new(context.realm(), native(text_decoder_decode))
        .name(js_string!("decode"))
        .length(1)
        .build();
    let _ = proto.set(
        js_string!("decode"),
        JsValue::from(decode_fn),
        false,
        context,
    );
    let encoding_get = accessor_fn(context, text_decoder_encoding_get);
    let _ = proto.define_property_or_throw(
        js_string!("encoding"),
        PropertyDescriptor::builder()
            .get(JsValue::from(encoding_get))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );
    constructor
}

pub(crate) fn text_decoder_constructor(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    Ok(JsValue::from(init.build()))
}

pub(crate) fn text_decoder_encoding_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!("utf-8")))
}

pub(crate) fn text_decoder_decode(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let input = args.first().cloned().unwrap_or(JsValue::undefined());
    if input.is_undefined() || input.is_null() {
        return Ok(JsValue::from(js_string!("")));
    }
    // Try to read as Uint8Array
    let obj = input
        .as_object()
        .ok_or_else(|| js_error("decode requires a BufferSource"))?;
    let len = obj
        .get(js_string!("length"), context)
        .ok()
        .and_then(|v| v.to_number(context).ok())
        .unwrap_or(0.0) as usize;
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        let b = obj
            .get(js_string!(i.to_string()), context)
            .ok()
            .and_then(|v| v.to_number(context).ok())
            .unwrap_or(0.0) as u8;
        bytes.push(b);
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(JsValue::from(js_string!(text.to_string())))
}

/// Installs History/location, Web Storage and MutationObserver (round 24).
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
