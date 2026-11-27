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
            host.push_console(ConsoleEntry {
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
            host.push_console(ConsoleEntry {
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
    node_list(context, nodes)
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
    node_list(context, nodes)
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

/// Collects connected elements to upgrade in shadow-including tree order.
/// WHATWG Custom Elements §4.2: upgrade reactions run for connected candidates only.
fn upgrade_candidates(host: &Host, name: &str) -> Vec<NodeId> {
    let mut candidates = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![host.document.root()];
    while let Some(node_id) = stack.pop() {
        if !visited.insert(node_id) {
            continue;
        }
        if let Some(node) = host.document.try_node(node_id)
            && let megi_core::NodeKind::Element(data) = &node.kind
            && data.namespace == Namespace::Html
            && data.local_name == name
            && !host.custom_element_upgraded.contains_key(&node_id)
        {
            candidates.push(node_id);
        }
        // LIFO: push light children first so shadow trees are visited before them.
        for child in host
            .document
            .children(node_id)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            stack.push(child);
        }
        if let Some(shadow) = host.shadow_roots.get(&node_id) {
            for child in host
                .document
                .children(*shadow)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
    }
    candidates
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
    // WHATWG Custom Elements §4.2: customized built-ins keep the extended local name;
    // autonomous definitions create elements named after the definition itself.
    let extends_name = args
        .get(2)
        .and_then(JsValue::as_string)
        .map(|text| text.to_std_string_escaped().to_ascii_lowercase())
        .filter(|text| !text.is_empty());
    let local_name = extends_name.clone().unwrap_or_else(|| name.clone());
    let handle = with_context_host(context, |host| {
        if let Some(existing) = host.custom_elements.get(&name) {
            return *existing;
        }
        let handle = host.custom_element_next_handle;
        host.custom_element_next_handle = host.custom_element_next_handle.wrapping_add(1);
        host.custom_elements.insert(name.clone(), handle);
        if let Some(extends) = extends_name.as_deref() {
            host.custom_element_extends
                .insert(name.clone(), extends.to_owned());
        }
        handle
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    context
        .global_object()
        .set(
            js_string!(format!("__megi_ce_{handle}")),
            JsValue::from(constructor.clone()),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    let candidates = with_context_host(context, |host| upgrade_candidates(host, &local_name))
        .unwrap_or_default();
    for node in candidates {
        let skip = with_context_host(context, |host| {
            if host.custom_element_upgraded.contains_key(&node) {
                return true;
            }
            host.custom_element_upgraded.insert(node, name.clone());
            host.custom_element_upgrade_target = Some(node);
            false
        })
        .unwrap_or(true);
        if skip {
            continue;
        }
        // WHATWG Custom Elements §4.2: upgrade reactions reuse the connected element.
        let _ = constructor.construct(&[], Some(&constructor), context);
        let _ = with_context_host(context, |host| host.custom_element_upgrade_target.take());
    }
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
