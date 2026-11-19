use super::*;
use boa_engine::Source;

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
    // #1972: browsers expose native constructors' `prototype` as non-writable and
    // non-configurable; instrumentation scripts branch on that descriptor.
    context
        .eval(Source::from_bytes(
            r#"["Event", "CustomEvent", "MessageEvent", "Element", "EventTarget",
                "HTMLLinkElement", "XMLHttpRequest"].forEach(function (name) {
              var fn = globalThis[name];
              if (typeof fn === "function" && Object.prototype.hasOwnProperty.call(fn, "prototype")) {
                var d = Object.getOwnPropertyDescriptor(fn, "prototype");
                if (d && d.writable) {
                  Object.defineProperty(fn, "prototype", {
                    value: d.value, writable: false, enumerable: false, configurable: false
                  });
                }
              }
            });"#,
        ))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}
