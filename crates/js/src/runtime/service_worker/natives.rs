//! Native bridges for Service Workers phase 1 and in-memory CacheStorage.
//!
//! WHATWG Service Workers §4/§5 and W3C Cache API §5/§5.4. All realm access
//! goes through the `Copy` [`SwOps`] capability slot installed in `HostDefined`.

use boa_engine::{JsValue, js_string, object::ObjectInitializer, property::Attribute};

use super::ops::{
    CachedResponse, SwRegistration, SwState, arg_string, is_same_origin, js_error, require_ops,
};

const MAX_SW_HEADERS: usize = 64;

/// Flattens the JS-side `Response` shim into a host-owned stored response.
fn response_from_args(
    args: &[JsValue],
    start: usize,
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<CachedResponse> {
    let status = args
        .get(start)
        .and_then(JsValue::as_number)
        .unwrap_or(200.0)
        .clamp(100.0, 599.0) as u16;
    let status_text = arg_string(args, start + 1, context)?;
    let url = arg_string(args, start + 2, context)?;
    let mut headers = Vec::new();
    if let Some(flat) = args.get(start + 3).and_then(JsValue::as_object) {
        let length = flat
            .get(js_string!("length"), context)?
            .as_number()
            .unwrap_or(0.0) as usize;
        for index in (0..length.min(MAX_SW_HEADERS * 2)).step_by(2) {
            let name = flat.get(js_string!(index.to_string()), context)?;
            let value = flat.get(js_string!((index + 1).to_string()), context)?;
            headers.push((
                name.to_string(context)?.to_std_string_escaped(),
                value.to_string(context)?.to_std_string_escaped(),
            ));
        }
    }
    let body = arg_string(args, start + 4, context)?;
    if body.len() > 8 * 1024 * 1024 {
        return Err(js_error("service worker response body limit exceeded"));
    }
    Ok(CachedResponse {
        status,
        status_text,
        url,
        headers,
        body: body.into_bytes(),
    })
}

/// WHATWG SW §4.1: `navigator.serviceWorker.register(script, {scope})`.
pub(crate) fn sw_register_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    let script = arg_string(args, 0, context)?;
    let mut scope = "/".to_string();
    if let Some(options) = args.get(1).and_then(JsValue::as_object)
        && let Ok(value) = options.get(js_string!("scope"), context)
        && !value.is_undefined()
    {
        scope = value
            .to_string(context)
            .map(|text| text.to_std_string_escaped())
            .unwrap_or(scope);
    }

    let (document_url, opaque) = (ops.document)(context);
    if opaque {
        return Err(js_error(
            "service workers require a non-opaque, secure context",
        ));
    }
    let script_url = (ops.resolve_url)(context, &script);
    let scope_url = (ops.resolve_url)(context, &scope);
    // WHATWG SW §4.1.1: script and scope share the client's origin.
    if !is_same_origin(&document_url, &script_url) || !is_same_origin(&document_url, &scope_url) {
        return Err(js_error(
            "service worker script and scope must be same-origin",
        ));
    }
    let source = (ops.load_script)(context, &script_url).map_err(js_error)?;
    if source.len() > 8 * 1024 * 1024 {
        return Err(js_error("service worker script exceeds byte cap"));
    }

    let worker_id = (ops.create_realm)(context).map_err(js_error)?;
    (ops.prepare_realm)(context, worker_id, &script_url).map_err(js_error)?;
    (ops.eval_realm)(context, worker_id, &source, "<service worker>").map_err(js_error)?;

    let mut registration = SwRegistration {
        script_url: script_url.clone(),
        scope: scope_url,
        state: SwState::Installing,
        worker_id,
    };

    // WHATWG SW §4.4/§4.5: install, then activate, each with a microtask drain.
    (ops.fire_lifecycle)(context, worker_id, "install").map_err(js_error)?;
    (ops.pump_realm)(context, worker_id).map_err(js_error)?;
    registration.state = SwState::Activated;
    (ops.fire_lifecycle)(context, worker_id, "activate").map_err(js_error)?;
    (ops.pump_realm)(context, worker_id).map_err(js_error)?;
    (ops.put_registration)(context, registration);

    // The JS-visible registration echoes the requested scope; the stored
    // registration keeps the resolved same-origin URL.
    let promise = boa_engine::object::builtins::JsPromise::new(
        move |resolvers, context| {
            let mut init = ObjectInitializer::new(context);
            init.property(
                js_string!("scope"),
                JsValue::from(js_string!(scope)),
                Attribute::all(),
            );
            init.property(
                js_string!("state"),
                JsValue::from(js_string!("activated")),
                Attribute::all(),
            );
            resolvers.resolve.call(
                &JsValue::undefined(),
                &[JsValue::from(init.build())],
                context,
            )?;
            Ok(JsValue::undefined())
        },
        context,
    )
    .map_err(|error| js_error(error.to_string()))?;
    Ok(JsValue::from(promise))
}

/// `navigator.serviceWorker.controller` (WHATWG SW §4.7).
pub(crate) fn sw_controller_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    let Some(registration) = (ops.registration)(context) else {
        return Ok(JsValue::null());
    };
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("state"),
        JsValue::from(js_string!(registration.state.as_str())),
        Attribute::all(),
    )
    .property(
        js_string!("scriptURL"),
        JsValue::from(js_string!(registration.script_url)),
        Attribute::all(),
    );
    Ok(JsValue::from(init.build()))
}

/// W3C Cache API §5.4: `cache.put(request, response)`.
pub(crate) fn cache_put_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    let name = arg_string(args, 0, context)?;
    let key = arg_string(args, 1, context)?;
    let response = response_from_args(args, 2, context)?;
    let stored = (ops.cache_put)(context, &name, &key, response);
    Ok(JsValue::from(stored))
}

/// W3C Cache API §5.4: `cache.match(request)` / §5 `caches.match(request)`.
pub(crate) fn cache_match_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    let name = arg_string(args, 0, context)?;
    let key = arg_string(args, 1, context)?;
    match (ops.cache_match)(context, &name, &key) {
        Some(response) => Ok(response.into_raw(context)),
        None => Ok(JsValue::undefined()),
    }
}

/// WHATWG SW §5.2: `FetchEvent.respondWith(response)` capture.
pub(crate) fn sw_capture_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    let response = response_from_args(args, 0, context)?;
    (ops.set_captured)(context, response);
    Ok(JsValue::undefined())
}

/// Returns the worker script URL so the SW realm can define `location`.
pub(crate) fn sw_script_url_native(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut boa_engine::Context,
) -> boa_engine::JsResult<JsValue> {
    let ops = require_ops(context)?;
    Ok(JsValue::from(js_string!((ops.realm_url)(context))))
}
