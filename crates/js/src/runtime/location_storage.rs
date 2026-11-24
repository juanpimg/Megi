use super::*;

pub(crate) fn url_parts(url: &str) -> (String, String, String, String, String) {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let protocol = if scheme.is_empty() {
        String::new()
    } else {
        format!("{scheme}:")
    };
    let (authority, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let origin = if scheme.is_empty() {
        String::new()
    } else {
        format!("{scheme}://{authority}")
    };
    let (path_query, hash) = tail.split_once('#').unwrap_or((tail, ""));
    let pathname = if scheme.is_empty() && rest.starts_with('/') {
        rest.split(['?', '#']).next().unwrap_or("").to_string()
    } else {
        format!("/{}", path_query.split('?').next().unwrap_or(""))
    };
    let search = path_query
        .split_once('?')
        .map(|(_, query)| format!("?{query}"))
        .unwrap_or_default();
    let hash = if hash.is_empty() {
        String::new()
    } else {
        format!("#{hash}")
    };
    (
        protocol,
        origin,
        authority.to_string(),
        pathname,
        format!("{search}\u{1f}{hash}"),
    )
}

pub(crate) fn current_location(
    context: &mut Context,
) -> JsResult<(String, String, String, String, String, String)> {
    let _ = context;
    let (url, opaque) = with_context_host(context, |host| (host.url.clone(), host.opaque_origin))
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    let (protocol, mut origin, host, pathname, rest) = url_parts(&url);
    if opaque {
        origin = "null".to_string();
    }
    let (search, hash) = rest.split_once('\u{1f}').unwrap_or(("", ""));
    Ok((
        protocol,
        origin,
        host,
        pathname,
        search.to_string(),
        hash.to_string(),
    ))
}

pub(crate) fn location_protocol_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.0)))
}
pub(crate) fn location_origin_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.1)))
}
pub(crate) fn location_host_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.2)))
}
/// `location.hostname` — the host without the port (#2245).
pub(crate) fn location_hostname_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let host = current_location(c)?.2;
    let hostname = host.split(':').next().unwrap_or(&host);
    Ok(JsValue::from(js_string!(hostname)))
}
pub(crate) fn location_pathname_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.3)))
}
pub(crate) fn location_search_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.4)))
}
pub(crate) fn location_hash_get(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!(current_location(c)?.5)))
}
pub(crate) fn location_href_get(
    _t: &JsValue,
    _a: &[JsValue],
    _c: &mut Context,
) -> JsResult<JsValue> {
    let url = with_context_host(_c, |host| host.url.clone())
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    Ok(JsValue::from(js_string!(url)))
}

pub(crate) fn navigate_location(
    args: &[JsValue],
    context: &mut Context,
    replace: bool,
) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let resolved = with_context_host(context, |host| host.resolve_url(&url));
    let Some(resolved) = resolved else {
        return Err(js_error("no active JavaScript host"));
    };
    let provider = ctx_or_mirror(
        context,
        |slot| slot.location.clone(),
        |host| host.location_provider.clone(),
    );
    if let Some(provider) = provider {
        provider(if replace {
            LocationAction::Replace(resolved.clone())
        } else {
            LocationAction::Assign(resolved.clone())
        });
    }
    with_context_host(context, |host| host.url = resolved);
    Ok(JsValue::undefined())
}

pub(crate) fn location_assign(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    navigate_location(args, context, false)
}
pub(crate) fn location_replace(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    navigate_location(args, context, true)
}
pub(crate) fn location_reload(
    _t: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some(provider) = ctx_or_mirror(
        context,
        |slot| slot.location.clone(),
        |host| host.location_provider.clone(),
    ) {
        provider(LocationAction::Reload);
    }
    Ok(JsValue::undefined())
}
pub(crate) fn location_href_set(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    navigate_location(args, context, false)
}

pub(crate) fn storage_kind(this: &JsValue, context: &mut Context) -> JsResult<StorageKind> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("storage receiver"))?;
    let value = object
        .get(js_string!("__megi_storage_kind"), context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(if value.to_boolean() {
        StorageKind::Session
    } else {
        StorageKind::Local
    })
}

/// Runs `f` with a borrow of the selected storage map (#1411).
///
/// The old implementation cloned the entire `StorageMap` on every read and discarded `f`
/// (`let _ = f;`), making every `getItem`/`key`/`length` O(n) over a session. Borrowing through
/// `host_ref` keeps lookups O(1) as documented.
pub(crate) fn storage_read<T>(
    this: &JsValue,
    context: &mut Context,
    f: impl FnOnce(&crate::host::StorageMap) -> T,
) -> JsResult<T> {
    let kind = storage_kind(this, context)?;
    let opaque = with_context_host(context, |host| host.opaque_origin).unwrap_or(false);
    if opaque {
        return Err(js_error(
            "SecurityError: failed to read Web Storage from an opaque origin",
        ));
    }
    with_context_host(context, |host| {
        let map = match kind {
            StorageKind::Local => &host.local_storage,
            StorageKind::Session => &host.session_storage,
        };
        f(map)
    })
    .ok_or_else(|| js_error("no active JavaScript host"))
}

pub(crate) fn persist_storage(kind: StorageKind, context: &mut Context) {
    let snapshot: Vec<(String, String)> = with_context_host(context, |host| match kind {
        StorageKind::Local => host
            .local_storage
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        StorageKind::Session => host
            .session_storage
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
    .unwrap_or_default();
    let saver = ctx_or_mirror(
        context,
        |slot| slot.storage_save.clone(),
        |host| {
            host.storage_provider
                .as_ref()
                .map(|provider| provider.save.clone())
        },
    );
    if let Some(saver) = saver {
        saver(kind, &snapshot);
    }
}

pub(crate) fn storage_get_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let key = arg_string(args, 0, context)?;
    storage_read(this, context, |map| {
        map.get(&key).map_or_else(JsValue::null, |value| {
            JsValue::from(js_string!(value.clone()))
        })
    })
}

pub(crate) fn storage_set_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let key = arg_string(args, 0, context)?;
    let value = arg_string(args, 1, context)?;
    let kind = storage_kind(this, context)?;
    let mut total: usize = storage_read(this, context, |current| {
        let mut total: usize = current.iter().map(|(k, v)| k.len() + v.len()).sum();
        if let Some(previous) = current.get(&key) {
            total = total.saturating_sub(key.len() + previous.len());
        }
        total
    })?;
    total = total.saturating_add(key.len() + value.len());
    if total > 1024 * 1024 {
        return Err(js_error(
            "QuotaExceededError: Web Storage limit of 1 MiB per realm exceeded",
        ));
    }
    with_context_host(context, |host| match kind {
        StorageKind::Local => host.local_storage.insert(key, value),
        StorageKind::Session => host.session_storage.insert(key, value),
    });
    persist_storage(kind, context);
    Ok(JsValue::undefined())
}

pub(crate) fn storage_remove_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if with_context_host(context, |host| host.opaque_origin).unwrap_or(false) {
        return Err(js_error(
            "SecurityError: failed to modify Web Storage from an opaque origin",
        ));
    }
    let key = arg_string(args, 0, context)?;
    let kind = storage_kind(this, context)?;
    with_context_host(context, |host| match kind {
        StorageKind::Local => host.local_storage.remove(&key),
        StorageKind::Session => host.session_storage.remove(&key),
    });
    persist_storage(kind, context);
    Ok(JsValue::undefined())
}

pub(crate) fn storage_clear(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if with_context_host(context, |host| host.opaque_origin).unwrap_or(false) {
        return Err(js_error(
            "SecurityError: failed to clear Web Storage from an opaque origin",
        ));
    }
    let kind = storage_kind(this, context)?;
    with_context_host(context, |host| match kind {
        StorageKind::Local => host.local_storage.clear(),
        StorageKind::Session => host.session_storage.clear(),
    });
    persist_storage(kind, context);
    Ok(JsValue::undefined())
}

pub(crate) fn storage_key(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = crate::events::webidl_uint32_index(args, context);
    storage_read(this, context, |map| {
        map.keys()
            .nth(index)
            .map_or_else(JsValue::null, |key| JsValue::from(js_string!(key.clone())))
    })
}

pub(crate) fn storage_length_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    storage_read(this, context, |map| JsValue::from(map.len() as f64))
}

#[cfg(test)]
mod storage_borrow_tests {
    use super::*;
    use std::cell::Cell;

    /// `storage_read` must actually invoke the caller's closure on a borrow of the selected
    /// map (#1411). The old implementation cloned the whole map and discarded `f`
    /// (`let _ = f;`), so this cell would never be set.
    #[test]
    pub(crate) fn storage_read_applies_the_closure_without_cloning() {
        let mut runtime = JsRuntime::new().expect("runtime");
        let local = runtime
            .context
            .global_object()
            .get(js_string!("localStorage"), &mut runtime.context)
            .expect("localStorage global");

        let called = Cell::new(false);
        let len = storage_read(&local, &mut runtime.context, |map| {
            called.set(true);
            map.len()
        })
        .expect("storage read");
        assert!(called.get(), "the storage closure must actually be invoked");
        assert_eq!(len, 0, "a fresh realm has no localStorage entries");
    }
}
