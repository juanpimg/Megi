use super::*;

impl JsRuntime {
    pub(crate) fn deliver_worker_message(&mut self, json: &str) -> Result<(), JsError> {
        let data = self.parse_json_value(json)?;
        self.context
            .global_object()
            .set(
                js_string!("__megi_worker_data"),
                data,
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        let script = r#"
(function () {
  var event = { data: __megi_worker_data };
  if (typeof onmessage === "function") { onmessage(event); }
  if (typeof __megi_worker_listeners !== "undefined") {
    for (var i = 0; i < __megi_worker_listeners.length; i++) { __megi_worker_listeners[i](event); }
  }
})();
"#;
        self.context
            .eval(Source::from_bytes(script))
            .map(|_| ())
            .map_err(|error| JsError::Runtime(error.to_string()))
    }
}

pub(crate) fn worker_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("worker receiver"))?;
    object
        .get(js_string!("__megi_worker"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|value| *value >= 0.0)
        .map(|value| value as u32)
        .ok_or_else(|| js_error("invalid Worker"))
}

pub(crate) fn worker_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let resolved = with_context_host(context, |host| host.resolve_url(&url))
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    let loader = ctx_or_mirror(
        context,
        |slot| slot.worker_loader.clone(),
        |host| host.worker_loader.clone(),
    )
    .ok_or_else(|| js_error("Worker scripts are not available in this host"))?;
    let source = loader(&resolved).map_err(|error| js_error(error.to_string()))?;
    let id = with_context_host(context, |host| {
        let existing = host.workers.iter().position(|worker| worker.is_none());
        match existing {
            Some(id) => Some(id),
            None if host.workers.len() < MAX_WORKERS => {
                host.workers.push(None);
                Some(host.workers.len() - 1)
            }
            None => None,
        }
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?
    .ok_or_else(|| js_error("too many Web Workers (limit 8)"))? as u32;
    let mut runtime = JsRuntime::new_worker(id).map_err(|error| js_error(error.to_string()))?;
    runtime
        .install_worker_globals()
        .map_err(|error| js_error(error.to_string()))?;
    let error = runtime.eval(&source, &resolved).err();
    with_context_host(context, |host| {
        host.workers[id as usize] = Some(WorkerState::new(runtime));
        if let Some(error) = error
            && let Some(worker) = host.workers[id as usize].as_mut()
        {
            worker.pending_error = Some(error.to_string());
        }
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    let mut init = ObjectInitializer::new(context);
    // Internal identity must not be writable by page scripts: a forged id used to
    // index host.workers out of bounds and abort the process (#1465).
    init.property(
        js_string!("__megi_worker"),
        JsValue::from(f64::from(id)),
        Attribute::default(),
    );
    init.function(native(worker_post_message), js_string!("postMessage"), 1);
    init.function(native(worker_terminate), js_string!("terminate"), 0);
    init.function(
        native(worker_add_listener),
        js_string!("addEventListener"),
        2,
    );
    init.function(
        native(worker_remove_listener),
        js_string!("removeEventListener"),
        2,
    );
    let worker_object = init.build();
    define_accessor(
        context,
        &worker_object,
        "onmessage",
        worker_onmessage_get,
        Some(worker_onmessage_set),
    )
    .map_err(|error| js_error(error.to_string()))?;
    define_accessor(
        context,
        &worker_object,
        "onerror",
        worker_onerror_get,
        Some(worker_onerror_set),
    )
    .map_err(|error| js_error(error.to_string()))?;
    Ok(JsValue::from(worker_object))
}

pub(crate) fn worker_post_message(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&value, context);
    let queued = with_context_host(context, |host| {
        let Some(Some(worker)) = host.workers.get_mut(id as usize) else {
            return false;
        };
        if worker.terminated || worker.to_worker.len() >= MAX_WORKER_MESSAGES {
            return false;
        }
        worker.to_worker.push_back(json);
        true
    })
    .unwrap_or(false);
    if !queued {
        return Err(js_error("worker is terminated or its queue is full"));
    }
    Ok(JsValue::undefined())
}

pub(crate) fn worker_terminate(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    with_context_host(context, |host| {
        // Free the slot so a new worker can reuse it (#1138). Never index directly:
        // a forged/mismatched handle must be a no-op, not a panic (#1465).
        if let Some(slot) = host.workers.get_mut(id as usize) {
            *slot = None;
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn worker_add_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    if event_type != "message" {
        return Ok(JsValue::undefined());
    }
    let callback = args.get(1).cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_context_host(context, |host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onmessage.push(slot);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn worker_remove_listener(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // #1745: EventTarget.removeEventListener must stop delivery by callback identity.
    let id = worker_handle(this, context)?;
    let event_type = arg_string(args, 0, context)?;
    if event_type != "message" {
        return Ok(JsValue::undefined());
    }
    let callback = args.get(1).cloned().unwrap_or_else(JsValue::undefined);
    let slots: Vec<u32> = with_context_host(context, |host| {
        host.workers
            .get(id as usize)
            .and_then(|worker| worker.as_ref())
            .map(|worker| worker.onmessage.clone())
    })
    .flatten()
    .unwrap_or_default();
    let retained: Vec<u32> = slots
        .into_iter()
        .filter(|slot| {
            root_get(context, "__megi_worker_callbacks", u64::from(*slot))
                .map(|stored| !stored.strict_equals(&callback))
                .unwrap_or(true)
        })
        .collect();
    with_context_host(context, |host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onmessage = retained;
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn worker_onmessage_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let slot = with_context_host(context, |host| {
        host.workers
            .get(id as usize)
            .and_then(|worker| worker.as_ref())
            .and_then(|worker| worker.onmessage.first().copied())
    })
    .flatten();
    match slot {
        Some(slot) => root_get(context, "__megi_worker_callbacks", u64::from(slot)),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn worker_onmessage_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let callback = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_context_host(context, |host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onmessage.clear();
            worker.onmessage.push(slot);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn worker_onerror_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // #1746: reflect the handler stored by `worker_onerror_set` (WebIDL §3.7.4).
    let id = worker_handle(this, context)?;
    let slot = with_context_host(context, |host| {
        host.workers
            .get(id as usize)
            .and_then(|worker| worker.as_ref())
            .and_then(|worker| worker.onerror)
    })
    .flatten();
    match slot {
        Some(slot) => root_get(context, "__megi_worker_callbacks", u64::from(slot)),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn worker_onerror_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = worker_handle(this, context)?;
    let callback = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let slot = root_push(context, "__megi_worker_callbacks", callback)?;
    with_context_host(context, |host| {
        if let Some(Some(worker)) = host.workers.get_mut(id as usize) {
            worker.onerror = Some(slot);
        }
    });
    Ok(JsValue::undefined())
}
