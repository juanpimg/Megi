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
    // Canvas 2D bindings reach realm state through this narrow capability slot
    // instead of borrowing `Host` from every native callback. The closures are
    // non-capturing function pointers; `Host` stays the single owner of state.
    crate::canvas::ops::CanvasOps::install(
        context,
        crate::canvas::ops::CanvasOps {
            resolve_handle: |context, handle| {
                host_ref(context)
                    .ok()
                    .and_then(|host| host.node_for_handle(handle).ok())
            },
            dimensions: |context, node| {
                let host = host_ref(context).ok()?;
                let element = host.document.try_node(node)?.as_element()?;
                let dimension = |name: &str, default: u32| -> u32 {
                    element
                        .get_attribute(name)
                        .and_then(|value| value.trim().parse::<u32>().ok())
                        .unwrap_or(default)
                        .min(crate::canvas::MAX_CANVAS_DIM)
                };
                Some((dimension("width", 300), dimension("height", 150)))
            },
            ensure_surface: |context, node, width, height| {
                with_context_host(context, |host| {
                    host.canvas_store.surface_for(node, width, height).is_some()
                })
                .unwrap_or(false)
            },
            handle_for: |context, node| {
                with_context_host(context, |host| host.handle_for(node)).and_then(Result::ok)
            },
            with_surface: |context, node, f| {
                let _ =
                    with_context_host(context, |host| host.canvas_store.with_surface_mut(node, f));
            },
            snapshot: |context, node| {
                with_context_host(context, |host| host.canvas_store.snapshot_surface(node))
                    .flatten()
            },
        },
    );
    install_gap_apis(context)?;
    use crate::runtime::service_worker::{
        CachedResponse, MAX_CACHE_ENTRIES, SwOps, SwRegistration, SwRequest,
    };

    let ops = SwOps {
        document: |context| {
            host_ref(context)
                .map(|host| (host.url.clone(), host.opaque_origin))
                .unwrap_or_default()
        },
        resolve_url: |context, url| {
            host_ref(context)
                .map(|host| host.resolve_url(url))
                .unwrap_or_else(|_| url.to_string())
        },
        load_script: |context, url| {
            let loader = ctx_or_mirror(
                context,
                |slot| slot.worker_loader.clone(),
                |host| host.worker_loader.clone(),
            );
            match loader {
                Some(loader) => loader(url).map_err(|error| error.to_string()),
                None => Err("service worker script loader is not available".to_owned()),
            }
        },
        create_realm: |context| {
            let id = {
                let mut host = host_cell(context).map_err(|error| error.to_string())?;
                let id = host.workers.len() as u32;
                host.workers.push(None);
                id
            };
            let mut worker = JsRuntime::new_worker(id).map_err(|error| error.to_string())?;
            if let Some(provider) = ctx_or_mirror(
                context,
                |slot| slot.fetch.clone(),
                |host| host.fetch_provider.clone(),
            ) {
                worker.set_fetch_provider(provider);
            }
            let _ = with_context_host(&mut worker.context, |host| host.worker_id = Some(id));
            host_cell(context)
                .map_err(|error| error.to_string())?
                .workers[id as usize] = Some(crate::runtime::WorkerState::new(worker));
            Ok(id)
        },
        prepare_realm: |context, id, script_url| {
            let ops = SwOps::from_context(context).ok_or("missing service worker ops")?;
            let mut host = host_cell(context).map_err(|error| error.to_string())?;
            let worker = host
                .workers
                .get_mut(id as usize)
                .and_then(|slot| slot.as_mut())
                .ok_or("missing service worker realm")?;
            let realm = &mut worker.runtime.context;
            let _ = with_context_host(realm, |host| host.sw_script_url = script_url.to_string());
            // The shared gap-apis prelude assumes a document realm; give the
            // worker realm minimal `window`/`Element` so its DOM shims skip.
            realm
                .eval(Source::from_bytes(
                    "globalThis.window = globalThis; globalThis.Element = function Element() {};",
                ))
                .map_err(|error| error.to_string())?;
            install_gap_apis(realm).map_err(|error| error.to_string())?;
            // The prelude calls `__megi_sw_script_url`, so the capability slot
            // must be live before it is evaluated.
            SwOps::install(realm, ops);
            crate::runtime::service_worker::setup_worker_scope(realm)
                .map_err(|error| error.to_string())?;
            Ok(())
        },
        eval_realm: |context, id, source, source_name| {
            let mut host = host_cell(context).map_err(|error| error.to_string())?;
            let worker = host
                .workers
                .get_mut(id as usize)
                .and_then(|slot| slot.as_mut())
                .ok_or("missing service worker realm")?;
            worker
                .runtime
                .eval(source, source_name)
                .map(|_| ())
                .map_err(|error| error.to_string())
        },
        pump_realm: |context, id| {
            let mut host = host_cell(context).map_err(|error| error.to_string())?;
            let worker = host
                .workers
                .get_mut(id as usize)
                .and_then(|slot| slot.as_mut())
                .ok_or("missing service worker realm")?;
            worker
                .runtime
                .run_microtasks()
                .map_err(|error| error.to_string())
        },
        fire_lifecycle: |context, id, event_type| {
            let mut host = host_cell(context).map_err(|error| error.to_string())?;
            let worker = host
                .workers
                .get_mut(id as usize)
                .and_then(|slot| slot.as_mut())
                .ok_or("missing service worker realm")?;
            crate::runtime::service_worker::fire_event(
                &mut worker.runtime.context,
                event_type,
                None,
            )
        },
        fire_fetch: |context, id, request: &SwRequest| {
            let mut host = host_cell(context).map_err(|error| error.to_string())?;
            let worker = host
                .workers
                .get_mut(id as usize)
                .and_then(|slot| slot.as_mut())
                .ok_or("missing service worker realm")?;
            crate::runtime::service_worker::fire_event(
                &mut worker.runtime.context,
                "fetch",
                Some(request),
            )
        },
        take_captured: |context, id| {
            let mut host = host_cell(context).ok()?;
            let worker = host.workers.get_mut(id as usize)?.as_mut()?;
            with_context_host(&mut worker.runtime.context, |host| host.sw_captured.take()).flatten()
        },
        registration: |context| {
            host_ref(context)
                .ok()
                .and_then(|host| host.service_worker.clone())
        },
        take_registration: |context| {
            with_context_host(context, |host| host.service_worker.take()).flatten()
        },
        put_registration: |context, registration: SwRegistration| {
            let _ = with_context_host(context, |host| host.service_worker = Some(registration));
        },
        cache_put: |context, name: &str, key: &str, response: CachedResponse| {
            with_context_host(context, |host| {
                let cache = host.cache_store.entry(name.to_owned()).or_default();
                if let Some(existing) = cache.iter_mut().find(|(stored, _)| stored == key) {
                    existing.1 = response;
                } else if cache.len() < MAX_CACHE_ENTRIES {
                    cache.push((key.to_owned(), response));
                } else {
                    return false;
                }
                true
            })
            .unwrap_or(false)
        },
        cache_match: |context, name: &str, key: &str| {
            let host = host_ref(context).ok()?;
            let mut names: Vec<&str> = if name.is_empty() {
                host.cache_store.keys().map(String::as_str).collect()
            } else {
                vec![name]
            };
            if name.is_empty() {
                names.sort_unstable();
            }
            names.into_iter().find_map(|cache_name| {
                host.cache_store.get(cache_name).and_then(|entries| {
                    entries
                        .iter()
                        .find(|(stored, _)| stored == key)
                        .map(|(_, response)| response.clone())
                })
            })
        },
        realm_url: |context| {
            host_ref(context)
                .map(|host| host.sw_script_url.clone())
                .unwrap_or_default()
        },
        set_captured: |context, response: CachedResponse| {
            let _ = with_context_host(context, |host| host.sw_captured = Some(response));
        },
    };
    SwOps::install(context, ops);
    crate::runtime::service_worker::install_main(context).map_err(JsError::Internal)?;
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
    // The JS layer needs the native registry to exist before it can wrap define/get.
    context
        .eval(boa_engine::Source::from_bytes(include_str!(
            "../gap_apis/ce_registry.js"
        )))
        .map(|_| ())
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
