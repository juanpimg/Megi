use super::*;

/// Extracts a child argument (wrapper → node id).
pub(crate) fn argument_node(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<NodeId> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    let object = value
        .as_object()
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    let handle = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    let handle = handle
        .as_number()
        .filter(|number| *number >= 0.0 && number.fract() == 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

pub(crate) fn node_clone_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let deep = args.first().and_then(JsValue::as_boolean).unwrap_or(false);
    // `cloneNode(true)` doubles the arena in one call, so the subtree size is charged before
    // the clone is allocated (#657).
    let allowed = with_context_host(context, |host| {
        let extra = if deep {
            host.document.descendants(node_id).count().max(1)
        } else {
            1
        };
        let total = host.document.len().saturating_add(extra);
        if total > host.max_dom_nodes {
            if !host.dom_budget_warned {
                host.dom_budget_warned = true;
                host.push_console(ConsoleEntry {
                    level: "warn".to_string(),
                    message: format!(
                        "cloneNode rejected: the DOM node budget ({} nodes) is exhausted",
                        host.max_dom_nodes
                    ),
                });
            }
            return false;
        }
        true
    })
    .unwrap_or(false);
    if !allowed {
        return Err(js_error(format!(
            "QuotaExceededError: DOM node budget exceeded ({} nodes)",
            with_context_host(context, |host| host.max_dom_nodes).unwrap_or(0)
        )));
    }
    let clone = with_context_host(context, |host| host.document.clone_node(node_id, deep))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .ok_or_else(|| js_error("NotFoundError: node not found"))?;
    wrap_node(context, clone)
}

pub(crate) fn node_append_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    // DOM insertion expands document fragments (their children are moved, the fragment
    // itself is never attached).
    let appended = with_context_host(context, |host| {
        host.document.insert_node(parent, child, None).is_ok()
    })
    .unwrap_or(false);
    if !appended {
        return Err(js_error("appendChild failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, child)
}

pub(crate) fn node_insert_before(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    // `insertBefore(node, null)` (or a missing reference) appends, per DOM.
    let reference = match args.get(1) {
        Some(value) if !value.is_null_or_undefined() => Some(argument_node(args, 1, context)?),
        _ => None,
    };
    if reference == Some(new_node) {
        // DOM spec: inserting a node before itself is a no-op.
        return wrap_node(context, new_node);
    }
    let inserted = with_context_host(context, |host| {
        host.document
            .insert_node(parent, new_node, reference)
            .is_ok()
    })
    .unwrap_or(false);
    if !inserted {
        return Err(js_error("insertBefore failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, new_node)
}

pub(crate) fn node_remove_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    let removed = with_context_host(context, |host| {
        host.document.remove_child(parent, child).is_ok()
    })
    .unwrap_or(false);
    if !removed {
        return Err(js_error("removeChild failed (node is not a child)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, child)
}

pub(crate) fn node_replace_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    let old_node = argument_node(args, 1, context)?;
    let replaced = with_context_host(context, |host| {
        host.document
            .replace_node(parent, new_node, old_node)
            .is_ok()
    })
    .unwrap_or(false);
    if !replaced {
        return Err(js_error("replaceChild failed"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, old_node)
}
