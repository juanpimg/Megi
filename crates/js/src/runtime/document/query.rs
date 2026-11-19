use super::*;

pub(crate) fn document_create_element(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let raw_name = arg_string(args, 0, context)?;
    if !is_valid_element_name(&raw_name) {
        return Err(js_error(format!(
            "InvalidCharacterError: `{raw_name}` is not a valid element name"
        )));
    }
    // DOM §4.5: `createElement` lowercases the name for an HTML document, so
    // `createElement("TEMPLATE")` still creates a template with its content fragment.
    let name = raw_name.to_ascii_lowercase();
    let node = with_context_host(context, |host| {
        host.document
            .create_element(Namespace::Html, name.clone(), Vec::new())
    });
    let wrapper = match node {
        Some(node_id) => wrap_node(context, node_id)?,
        None => return Err(js_error("no active JavaScript host")),
    };
    // Custom Elements upgrade (#1541): if a constructor is registered for this tag, invoke it
    // with the new element as `this` so it can attach a shadow root / set up state.
    let handle =
        with_context_host(context, |host| host.custom_elements.get(&name).copied()).flatten();
    if let Some(handle) = handle {
        let constructor = context
            .global_object()
            .get(js_string!(format!("__megi_ce_{handle}")), context)
            .ok()
            .and_then(|value| value.as_callable());
        if let Some(constructor) = constructor {
            let _ = constructor.call(&wrapper, &[], context);
        }
    }
    Ok(wrapper)
}

pub(crate) fn document_create_text_node(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let text = arg_string(args, 0, context)?;
    let node = with_context_host(context, |host| host.document.create_text_node(&text));
    match node {
        Some(node_id) => wrap_node(context, node_id),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn document_current_script(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = with_context_host(context, |host| host.current_script).flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_element(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = with_context_host(context, |host| host.document.document_element()).flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_body(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = with_context_host(context, |host| host.document.body()).flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_head(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = with_context_host(context, |host| host.document.head()).flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_cookie_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (opaque, url, fallback) = with_context_host(context, |host| {
        (
            host.opaque_origin,
            host.url.clone(),
            host.cookies_fallback.clone(),
        )
    })
    .unwrap_or((true, String::new(), Vec::new()));
    if opaque {
        return Ok(JsValue::from(js_string!("")));
    }
    let getter = ctx_or_mirror(
        context,
        |slot| slot.cookie_get.clone(),
        |host| {
            host.cookie_provider
                .as_ref()
                .map(|provider| provider.get.clone())
        },
    );
    let cookie_str = if let Some(getter) = getter {
        getter(&url)
    } else {
        fallback
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    Ok(JsValue::from(js_string!(cookie_str)))
}

pub(crate) fn document_cookie_set(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = arg_string(args, 0, context).unwrap_or_default();
    let setter = ctx_or_mirror(
        context,
        |slot| slot.cookie_set.clone(),
        |host| {
            host.cookie_provider
                .as_ref()
                .map(|provider| provider.set.clone())
        },
    );
    let (opaque, url) = with_context_host(context, |host| (host.opaque_origin, host.url.clone()))
        .unwrap_or((true, String::new()));
    if opaque {
        return Ok(JsValue::undefined());
    }
    if let Some(setter) = setter {
        setter(&url, &value);
        return Ok(JsValue::undefined());
    }
    let _ = with_context_host(context, |host| {
        let mut segments = value.split(';');
        let pair = segments.next().unwrap_or("").trim();
        let (name, val) = match pair.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (pair, ""),
        };
        if name.is_empty() {
            return;
        }
        let mut is_http_only = false;
        let mut delete = false;
        for attr in segments {
            let trimmed = attr.trim();
            if trimmed.eq_ignore_ascii_case("httponly") {
                is_http_only = true;
            } else if let Some((k, v)) = trimmed.split_once('=')
                && k.trim().eq_ignore_ascii_case("max-age")
                && let Ok(secs) = v.trim().parse::<i64>()
                && secs <= 0
            {
                delete = true;
            }
        }
        if is_http_only {
            return;
        }
        if delete {
            host.cookies_fallback
                .retain(|(existing, _)| existing != name);
        } else if let Some((_, existing_val)) = host
            .cookies_fallback
            .iter_mut()
            .find(|(existing, _)| existing == name)
        {
            *existing_val = val.to_owned();
        } else {
            host.cookies_fallback
                .push((name.to_owned(), val.to_owned()));
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn document_title_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let title = with_context_host(_context, |host| host.title.clone()).unwrap_or_default();
    Ok(JsValue::from(js_string!(title)))
}

pub(crate) fn document_title_set(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let title = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        host.title = title.clone();
        // #1645: the window/tab title is composed from the DOM <title> element. Update (or
        // create) it so the assignment has an observable effect beyond the internal field.
        let existing = host
            .document
            .get_elements_by_tag_name("title")
            .into_iter()
            .next();
        if let Some(node) = existing {
            let _ = host.document.set_text_content(node, &title);
        } else {
            let title_node = host
                .document
                .create_element(Namespace::Html, "title", Vec::new());
            let text_node = host.document.create_text_node(&title);
            host.document.append_child(title_node, text_node);
            let parent = host
                .document
                .head()
                .or_else(|| host.document.document_element());
            if let Some(parent) = parent {
                host.document.append_child(parent, title_node);
            }
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn document_url(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let url = with_context_host(_context, |host| host.url.clone()).unwrap_or_default();
    Ok(JsValue::from(js_string!(url)))
}
