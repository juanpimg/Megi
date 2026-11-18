use super::*;

// ---------------------------------------------------------------------------
// Timers

// ---------------------------------------------------------------------------
// History, location, Web Storage and MutationObserver (round 24)
// ---------------------------------------------------------------------------

pub(crate) fn root_array(context: &mut Context, name: &str, create: bool) -> JsResult<JsObject> {
    let global = context.global_object();
    let value = global
        .get(js_string!(name), context)
        .map_err(|error| js_error(error.to_string()))?;
    // Defensively type-check: a script may have replaced the rooted table with a
    // non-array value. Treat that as "missing" and recreate it (#644).
    if let Some(array) = value.as_object()
        && array
            .get(js_string!("length"), context)
            .ok()
            .and_then(|v| v.as_number())
            .is_some()
    {
        return Ok(array.clone());
    }
    let array = JsArray::from_iter([], context);
    global
        .set(
            js_string!(name),
            JsValue::from(array.clone()),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    let _ = create;
    Ok(JsObject::from(array))
}

pub(crate) fn root_push(context: &mut Context, name: &str, value: JsValue) -> JsResult<u32> {
    let array = root_array(context, name, true)?;
    let length = array
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u32;
    array
        .set(js_string!(length.to_string()), value, false, context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(length)
}

pub(crate) fn root_get(context: &mut Context, name: &str, index: u64) -> JsResult<JsValue> {
    let array = root_array(context, name, false)?;
    array
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn root_set(
    context: &mut Context,
    name: &str,
    index: u64,
    value: JsValue,
) -> JsResult<()> {
    let array = root_array(context, name, true)?;
    array
        .set(js_string!(index.to_string()), value, false, context)
        .map(|_| ())
        .map_err(|error| js_error(error.to_string()))
}

/// Serializes a JS value to JSON for the history provider (falls back to stringification).
pub(crate) fn json_state(value: &JsValue, context: &mut Context) -> String {
    let json = context
        .global_object()
        .get(js_string!("JSON"), context)
        .ok()
        .and_then(|value| value.as_object());
    if let Some(json) = json
        && let Ok(stringify) = json.get(js_string!("stringify"), context)
        && let Some(function) = stringify.as_object()
        && let Ok(result) =
            function.call(&JsValue::undefined(), std::slice::from_ref(value), context)
        && let Ok(text) = result.to_string(context)
    {
        return text.to_std_string_escaped();
    }
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .unwrap_or_else(|_| "null".to_string())
}

pub(crate) fn history_push_state(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let state = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&state, context);
    if json.len() > MAX_HISTORY_STATE_BYTES {
        return Err(js_error(format!(
            "QuotaExceededError: history state limit of 1 MiB exceeded ({} bytes)",
            json.len()
        )));
    }
    let url = args.get(2).and_then(|value| {
        value
            .to_string(context)
            .ok()
            .map(|text| text.to_std_string_escaped())
            .filter(|text| !text.is_empty())
    });
    let resolved = with_context_host(context, |host| {
        url.as_deref().map(|url| host.resolve_url(url))
    });
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    let provider = ctx_or_mirror(
        context,
        |slot| slot.history.clone(),
        |host| host.history_provider.clone(),
    );
    let applied = if let Some(provider) = provider {
        provider(HistoryAction::Push {
            state: json.clone(),
            url: resolved.clone(),
        });
        true
    } else {
        false
    };
    if applied {
        root_set(context, "__megi_history_state", 0, state)?;
        with_context_host(context, |host| {
            host.history_length = host.history_length.saturating_add(1);
            host.history_state = json.clone();
            if let Some(url) = &resolved {
                host.url = url.clone();
            }
        });
    }
    Ok(JsValue::undefined())
}

pub(crate) fn history_replace_state(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let state = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&state, context);
    if json.len() > MAX_HISTORY_STATE_BYTES {
        return Err(js_error(format!(
            "QuotaExceededError: history state limit of 1 MiB exceeded ({} bytes)",
            json.len()
        )));
    }
    let url = args.get(2).and_then(|value| {
        value
            .to_string(context)
            .ok()
            .map(|text| text.to_std_string_escaped())
            .filter(|text| !text.is_empty())
    });
    let resolved = with_context_host(context, |host| {
        url.as_deref().map(|url| host.resolve_url(url))
    });
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    let provider = ctx_or_mirror(
        context,
        |slot| slot.history.clone(),
        |host| host.history_provider.clone(),
    );
    let applied = if let Some(provider) = provider {
        provider(HistoryAction::Replace {
            state: json.clone(),
            url: resolved.clone(),
        });
        true
    } else {
        false
    };
    if applied {
        root_set(context, "__megi_history_state", 0, state)?;
        with_context_host(context, |host| {
            host.history_state = json.clone();
            if let Some(url) = &resolved {
                host.url = url.clone();
            }
        });
    }
    Ok(JsValue::undefined())
}

pub(crate) fn history_back(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(provider) = ctx_or_mirror(
        context,
        |slot| slot.history.clone(),
        |host| host.history_provider.clone(),
    ) {
        provider(HistoryAction::Back);
    }
    Ok(JsValue::undefined())
}

pub(crate) fn history_forward(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(provider) = ctx_or_mirror(
        context,
        |slot| slot.history.clone(),
        |host| host.history_provider.clone(),
    ) {
        provider(HistoryAction::Forward);
    }
    Ok(JsValue::undefined())
}

pub(crate) fn history_go(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let delta = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .map(|number| number as i64)
        .unwrap_or(0);
    if let Some(provider) = ctx_or_mirror(
        context,
        |slot| slot.history.clone(),
        |host| host.history_provider.clone(),
    ) {
        provider(HistoryAction::Go(delta));
    }
    Ok(JsValue::undefined())
}

pub(crate) fn history_length_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let _ = context;
    Ok(JsValue::from(
        with_context_host(context, |host| host.history_length).unwrap_or(1),
    ))
}

pub(crate) fn history_state_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = root_get(context, "__megi_history_state", 0)?;
    if value.is_undefined() {
        Ok(JsValue::null())
    } else {
        Ok(value)
    }
}
