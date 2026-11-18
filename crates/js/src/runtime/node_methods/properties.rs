use super::*;

pub(crate) fn boolean_attribute_get(
    this: &JsValue,
    name: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let present = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .is_some_and(|element| element.get_attribute(name).is_some())
    })
    .unwrap_or(false);
    Ok(JsValue::from(present))
}

pub(crate) fn boolean_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let enabled = args.first().is_some_and(JsValue::to_boolean);
    let result = with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        if enabled {
            element.set_attribute(DomAttribute::new(name, ""), true);
        } else {
            element.remove_attribute(name);
        }
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_open_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "open", context)
}

pub(crate) fn node_open_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "open", args, context)
}

pub(crate) fn node_checked_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "checked", context)
}

pub(crate) fn node_checked_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if !host.initial_control_checked.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.get_attribute("checked").is_some());
            host.initial_control_checked.insert(node_id, initial);
        }
    });
    boolean_attribute_set(this, "checked", args, context)
}

pub(crate) fn node_disabled_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "disabled", context)
}

pub(crate) fn node_disabled_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "disabled", args, context)
}

pub(crate) fn node_selected_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "selected", context)
}

pub(crate) fn node_selected_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if !host.initial_control_checked.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.get_attribute("selected").is_some());
            host.initial_control_checked.insert(node_id, initial);
        }
    });
    boolean_attribute_set(this, "selected", args, context)
}

pub(crate) fn node_options(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let options: Vec<NodeId> = with_context_host(context, |host| {
        host.document
            .descendants(node_id)
            .filter(|child| {
                host.document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|element| element.local_name == "option")
            })
            .collect()
    })
    .unwrap_or_default();
    node_list(context, options)
}

pub(crate) fn node_set_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let value = arg_string(args, 1, context)?;
    let old_value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    let result = with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        element.set_attribute(DomAttribute::new(name.clone(), value), true);
        if matches!(name.as_str(), "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => {
            note_mutation(
                MutationKind::Attributes,
                node_id,
                Some(name),
                old_value,
                context,
            );
            Ok(JsValue::undefined())
        }
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_get_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .map(str::to_owned)
    })
    .flatten();
    match value {
        Some(value) => Ok(JsValue::from(js_string!(value))),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn node_remove_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let old_value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element
                .attributes
                .retain(|attribute| attribute.local_name != name);
        }
        if matches!(name.as_str(), "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
    });
    note_mutation(
        MutationKind::Attributes,
        node_id,
        Some(name),
        old_value,
        context,
    );
    Ok(JsValue::undefined())
}

pub(crate) fn node_has_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let has = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .is_some()
    })
    .unwrap_or(false);
    Ok(JsValue::from(has))
}
