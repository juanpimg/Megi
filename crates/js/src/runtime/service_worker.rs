//! Bindings for Service Workers phase 1 (M4) built on a `Copy` capability slot.
//!
//! Like `CanvasOps`, this module never names `Host`, `host_cell` or `JsRuntime`:
//! the implementations of [`SwOps`] are installed by `runtime::globals::install`
//! as non-capturing function pointers. This keeps the bindings out of the
//! pre-existing `events → host → runtime` cycle.
//!
//! WHATWG Service Workers §4 (life cycle), §5 (global scope, `respondWith`);
//! W3C Cache API §5 (CacheStorage) / §5.4 (Cache).

use boa_engine::{
    Context, JsValue, NativeFunction, Source, js_string, object::FunctionObjectBuilder,
    object::ObjectInitializer, property::Attribute,
};

pub(crate) use ops::{CachedResponse, SwOps, SwRegistration, SwRequest, SwState, is_same_origin};

mod natives;
pub(crate) mod ops;
mod prelude;

pub(crate) use ops::*;

/// Installs `navigator.serviceWorker` in a document realm (WHATWG SW §4.1).
pub(crate) fn install_main(context: &mut Context) -> Result<(), String> {
    let navigator = context
        .global_object()
        .get(js_string!("navigator"), context)
        .map_err(|error| error.to_string())?;
    let Some(navigator) = navigator.as_object() else {
        return Ok(());
    };
    let controller = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_fn_ptr(natives::sw_controller_get),
    )
    .name(js_string!("get controller"))
    .length(0)
    .build();
    let mut sw = ObjectInitializer::new(context);
    sw.function(
        NativeFunction::from_fn_ptr(natives::sw_register_native),
        js_string!("register"),
        1,
    );
    sw.accessor(
        js_string!("controller"),
        Some(controller),
        None,
        Attribute::all(),
    );
    navigator
        .set(
            js_string!("serviceWorker"),
            JsValue::from(sw.build()),
            false,
            context,
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Installs the narrow native bridges and evaluates the SW global-scope prelude.
pub(crate) fn setup_worker_scope(context: &mut Context) -> Result<(), String> {
    for (name, function, length) in [
        (
            "__megi_sw_script_url",
            natives::sw_script_url_native
                as fn(&JsValue, &[JsValue], &mut Context) -> boa_engine::JsResult<JsValue>,
            0,
        ),
        ("__megi_cache_put", natives::cache_put_native, 7),
        ("__megi_cache_match", natives::cache_match_native, 2),
        ("__megi_sw_capture", natives::sw_capture_native, 5),
    ] {
        let built =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(function))
                .name(js_string!(name))
                .length(length)
                .build();
        context
            .global_object()
            .set(js_string!(name), JsValue::from(built), false, context)
            .map(|_| ())
            .map_err(|error| error.to_string())?;
    }
    context
        .eval(Source::from_bytes(prelude::SW_PRELUDE))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Fires a worker event by calling its prelude dispatcher with a payload.
pub(crate) fn fire_event(
    context: &mut Context,
    event_type: &str,
    request: Option<&SwRequest>,
) -> Result<(), String> {
    let headers = headers_value(request, context);
    let payload = ObjectInitializer::new(context)
        .property(
            js_string!("url"),
            JsValue::from(js_string!(
                request.map_or_else(String::new, |r| r.url.clone())
            )),
            Attribute::all(),
        )
        .property(
            js_string!("method"),
            JsValue::from(js_string!(
                request.map_or_else(String::new, |r| r.method.clone())
            )),
            Attribute::all(),
        )
        .property(js_string!("headers"), headers, Attribute::all())
        .build();
    let fire = context
        .global_object()
        .get(js_string!("__megi_sw_fire"), context)
        .map_err(|error| error.to_string())?
        .as_callable();
    if let Some(fire) = fire {
        fire.call(
            &JsValue::undefined(),
            &[
                JsValue::from(js_string!(event_type)),
                JsValue::from(payload),
            ],
            context,
        )
        .map(|_| ())
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn headers_value(request: Option<&SwRequest>, context: &mut Context) -> JsValue {
    use boa_engine::object::builtins::JsArray;
    let mut pairs = Vec::new();
    if let Some(request) = request {
        for (name, value) in &request.headers {
            pairs.push(JsValue::from(JsArray::from_iter(
                [
                    JsValue::from(js_string!(name.clone())),
                    JsValue::from(js_string!(value.clone())),
                ],
                context,
            )));
        }
    }
    JsValue::from(JsArray::from_iter(pairs, context))
}

/// Runs a same-origin GET through the active worker's `fetch` event.
pub(crate) fn try_fetch(context: &mut Context, request: &SwRequest) -> Option<CachedResponse> {
    if request.method != "GET" {
        return None;
    }
    let ops = SwOps::from_context(context)?;
    let (document_url, opaque) = (ops.document)(context);
    if opaque || !is_same_origin(&document_url, &request.url) {
        return None;
    }
    let registration = (ops.take_registration)(context)?;
    // WHATWG SW §4.1: only requests inside the worker's scope are controlled.
    if registration.state != SwState::Activated || !request.url.starts_with(&registration.scope) {
        (ops.put_registration)(context, registration);
        return None;
    }
    let captured = dispatch_fetch(context, ops, &registration, request);
    (ops.put_registration)(context, registration);
    captured
}

fn dispatch_fetch(
    context: &mut Context,
    ops: SwOps,
    registration: &SwRegistration,
    request: &SwRequest,
) -> Option<CachedResponse> {
    let id = registration.worker_id;
    (ops.fire_fetch)(context, id, request).ok()?;
    (ops.pump_realm)(context, id).ok()?;
    (ops.take_captured)(context, id)
}
