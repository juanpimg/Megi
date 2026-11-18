use super::*;

// ---------------------------------------------------------------------------
// document methods/accessors
// ---------------------------------------------------------------------------

/// `document.readyState` — `loading` during evaluation; the host updates it with
/// [`JsRuntime::set_ready_state`].
pub(crate) fn document_ready_state(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let state = with_context_host(_context, |host| host.ready_state.clone())
        .unwrap_or_else(|| "loading".to_string());
    Ok(JsValue::from(js_string!(state)))
}

/// `document.close()` — no-op kept for script compatibility (parsing is host-driven).
pub(crate) fn document_close(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

/// Charges `bytes` of markup parsing against the document's cumulative budget.
///
/// `document.write`, `innerHTML`, `outerHTML` and `insertAdjacentHTML` all parse
/// attacker-controlled markup; each parse is bounded per call, but a script loop could
/// otherwise force thousands of large parses. The byte budget is shared with the node cap
/// enforced by the parser ([`Host::max_document_write_nodes`]). Returns `false` when the
/// assignment must be ignored.
pub(crate) fn charge_document_parse(host: &mut Host, bytes: usize, sink: &str) -> bool {
    let total = host.document_write_bytes.saturating_add(bytes);
    if total > host.max_document_write_bytes || host.document.len() >= host.max_document_write_nodes
    {
        if !host.document_write_warned {
            host.document_write_warned = true;
            host.console.push_back(ConsoleEntry {
                level: "warn".to_string(),
                message: format!(
                    "{sink} ignored: the cumulative document budget ({} bytes / {} nodes) is exhausted",
                    host.max_document_write_bytes, host.max_document_write_nodes
                ),
            });
        }
        host.document_write_bytes = total;
        return false;
    }
    host.document_write_bytes = total;
    true
}

/// `document.writeln` is `write` plus a trailing newline (#1620).
pub(crate) fn document_writeln(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let text = arg_string(args, 0, context)?;
    let with_newline = format!("{text}\n");
    let mut new_args = args.to_vec();
    if new_args.is_empty() {
        new_args.push(JsValue::undefined());
    }
    new_args[0] = JsValue::from(js_string!(with_newline));
    document_write(this, &new_args, context)
}

pub(crate) fn document_write(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    const MAX_DOCUMENT_WRITE_INPUT: usize = 16 * 1024 * 1024;
    let text = arg_string(args, 0, context)?;
    if text.len() > MAX_DOCUMENT_WRITE_INPUT {
        return Err(js_error(format!(
            "document.write input exceeds the per-call limit of {} bytes",
            MAX_DOCUMENT_WRITE_INPUT
        )));
    }
    with_context_host(context, |host| {
        if !charge_document_parse(host, text.len(), "document.write") {
            return;
        }
        let options = megi_html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..megi_html::ParseOptions::default()
        };
        let violations =
            megi_html::document_write_with_options(&mut host.document, &text, false, &options);
        // Surface parser truncation to script instead of silently discarding it (#431).
        if !violations.is_empty() {
            host.console.push_back(ConsoleEntry {
                level: "warn".to_string(),
                message: format!(
                    "document.write: {} parse limit violation(s) ({} node(s), {} depth, {} text bytes)",
                    violations.total(),
                    violations.nodes_exceeded,
                    violations.depth_exceeded,
                    violations.text_bytes_dropped,
                ),
            });
        }
        let target = host
            .document
            .body()
            .or_else(|| host.document.document_element())
            .unwrap_or_else(|| host.document.root());
        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target,
            name: None,
            old_value: None,
        });
    });
    Ok(JsValue::undefined())
}

pub(crate) fn document_get_element_by_id(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let id = arg_string(args, 0, context)?;
    let node = with_context_host(context, |host| host.document.get_element_by_id(&id)).flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_get_elements_by_tag_name(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?;
    let nodes = with_context_host(context, |host| {
        host.document.get_elements_by_tag_name(&name)
    })
    .unwrap_or_default();
    node_list(context, nodes)
}

pub(crate) fn document_get_elements_by_class_name(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let class = arg_string(args, 0, context)?;
    let nodes = with_context_host(context, |host| {
        host.document.get_elements_by_class_name(&class)
    })
    .unwrap_or_default();
    wrap_list(context, nodes)
}

pub(crate) fn document_query_selector(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let selector = arg_string(args, 0, context)?;
    let node = with_context_host(context, |host| {
        megi_css::query::query_selector(&host.document, &selector)
    })
    .flatten();
    wrap_optional(context, node)
}

pub(crate) fn document_query_selector_all(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let selector = arg_string(args, 0, context)?;
    let nodes = with_context_host(context, |host| {
        megi_css::query::query_selector_all(&host.document, &selector)
    })
    .unwrap_or_default();
    wrap_list(context, nodes)
}

pub(crate) fn document_create_document_fragment(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let fragment = with_context_host(context, |host| host.document.create_document_fragment())
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    wrap_node(context, fragment)
}

pub(crate) fn is_valid_element_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let valid_start = first.is_ascii_alphabetic() || first == '_' || first == ':';
    if !valid_start {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | ':'))
}

/// `customElements.define(name, constructor)` (#1541): registers a constructor for a custom tag.
/// The name must contain a hyphen (Custom Elements spec). The constructor is stored on the global
/// object under an opaque key so `createElement` can invoke it to upgrade new instances.
pub(crate) fn custom_elements_define(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?.to_ascii_lowercase();
    if !name.contains('-') || !is_valid_element_name(&name) {
        return Err(js_error(format!(
            "NotSupportedError: `{name}` is not a valid custom element name"
        )));
    }
    let constructor = args
        .get(1)
        .and_then(JsValue::as_callable)
        .ok_or_else(|| js_error("TypeError: custom element constructor must be callable"))?
        .clone();
    let handle = with_context_host(context, |host| {
        if let Some(existing) = host.custom_elements.get(&name) {
            return *existing;
        }
        let handle = host.custom_element_next_handle;
        host.custom_element_next_handle = host.custom_element_next_handle.wrapping_add(1);
        host.custom_elements.insert(name.clone(), handle);
        handle
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    context
        .global_object()
        .set(
            js_string!(format!("__megi_ce_{handle}")),
            JsValue::from(constructor),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    Ok(JsValue::undefined())
}

/// `customElements.get(name)` — returns the registered constructor or `undefined`.
pub(crate) fn custom_elements_get(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?.to_ascii_lowercase();
    let handle =
        with_context_host(context, |host| host.custom_elements.get(&name).copied()).flatten();
    match handle {
        Some(handle) => context
            .global_object()
            .get(js_string!(format!("__megi_ce_{handle}")), context)
            .map_err(|error| js_error(error.to_string())),
        None => Ok(JsValue::undefined()),
    }
}

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

#[cfg(test)]
mod document_write_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn document_write_stops_at_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_bytes = 4 * 1024;
        runtime
            .eval(
                "var s = 'A'.repeat(1024);
                 for (var i = 0; i < 6; i++) { document.write(s); }",
                "write_budget.js",
            )
            .expect("document.write loop");
        let console = runtime.take_console();
        let document = runtime.take_document();
        let body = document.body().expect("body");
        assert_eq!(
            document.text_content(body).len(),
            4 * 1024,
            "only the writes that fit the budget are parsed into the document"
        );
        assert!(
            console.iter().any(|entry| {
                entry.level == "warn" && entry.message.contains("document.write ignored")
            }),
            "the exceeded budget must be reported: {console:?}"
        );
    }

    #[test]
    pub(crate) fn document_write_reports_parser_limit_violations() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 64;
        runtime
            .eval(
                "document.write('<i></i>'.repeat(100));",
                "nodes_violation.js",
            )
            .expect("write");
        let console = runtime.take_console();
        assert!(
            console.iter().any(|entry| {
                entry.level == "warn"
                    && entry.message.contains("parse limit violation")
                    && entry.message.contains("node(s)")
            }),
            "parser truncation must be visible to script: {console:?}"
        );
    }
}

#[cfg(test)]
mod document_write_node_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn document_write_node_budget_is_enforced_and_warned_once() {
        let mut runtime = JsRuntime::new().expect("runtime");
        // The parser must use the host's node cap, not the 1M default: 100 elements with a
        // 64-node budget leave ~62 nodes.
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 64;
        runtime
            .eval("document.write('<i></i>'.repeat(100));", "nodes.js")
            .expect("first write");
        let after_first = runtime.document().len();
        assert!(
            after_first <= 64,
            "the write parser stops at the host node cap: {after_first} nodes"
        );
        // Host-side rejection: a cap below the current size rejects every write; the
        // warning must appear exactly once.
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_nodes = 1;
        runtime
            .eval(
                "for (var j = 0; j < 5; j++) { document.write('<b></b>'); }",
                "nodes.js",
            )
            .expect("rejected writes");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("document.write ignored"))
            .count();
        assert_eq!(warnings, 1, "the rejection is reported once: {console:?}");
        let document = runtime.take_document();
        assert_eq!(
            document.len(),
            after_first,
            "rejected writes add nothing to the document"
        );
    }
}

#[cfg(test)]
mod markup_parse_budget_tests {
    use super::*;

    #[test]
    pub(crate) fn innerhtml_is_charged_against_the_cumulative_budget() {
        let mut runtime = JsRuntime::new().expect("runtime");
        host_cell(&mut runtime.context)
            .expect("host installed")
            .max_document_write_bytes = 64;
        runtime
            .eval("document.body.innerHTML = '<b>first</b>';", "inner.js")
            .expect("first assignment");
        runtime
            .eval(
                "document.body.innerHTML = '<b>' + 'x'.repeat(200) + '</b>';",
                "inner.js",
            )
            .expect("over-budget assignment");
        let text = {
            let document = runtime.document();
            let body = document.body().expect("body");
            document.text_content(body)
        };
        assert_eq!(text, "first", "the over-budget assignment is ignored");
        let console = runtime.take_console();
        let warnings = console
            .iter()
            .filter(|entry| entry.message.contains("innerHTML assignment ignored"))
            .count();
        assert_eq!(warnings, 1, "reported once: {console:?}");
    }
}
