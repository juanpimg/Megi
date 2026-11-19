use super::*;

/// Sets/clears the `open` attribute on a dialog element (#1546).
pub(crate) fn dialog_open(this: &JsValue, open: bool, context: &mut Context) -> JsResult<()> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut());
        if let Some(element) = element {
            if open {
                element.set_attribute(DomAttribute::new("open", ""), true);
            } else {
                element.attributes.retain(|attr| attr.local_name != "open");
            }
        }
    });
    Ok(())
}

/// `HTMLDialogElement.show()` / `showModal()` (#1546). The engine has no separate top-layer yet,
/// so both reveal the dialog by setting `open`; `showModal()` additionally records the modal
/// state on the element so scripts and the shell can observe which dialog is modal.
pub(crate) fn node_dialog_show(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    dialog_open(this, true, context)?;
    let _ = crate::events::dispatch_event_type(this, "toggle", false, false, context);
    Ok(JsValue::undefined())
}

pub(crate) fn node_dialog_show_modal(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    dialog_open(this, true, context)?;
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("data-megi-modal", ""), true);
        }
    });
    let _ = crate::events::dispatch_event_type(this, "toggle", false, false, context);
    Ok(JsValue::undefined())
}

/// `HTMLDialogElement.close(returnValue)` (#1546): hides the dialog, stores its return value and
/// fires a `close` event.
pub(crate) fn node_dialog_close(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let return_value = args
        .first()
        .map(|value| {
            value
                .to_string(context)
                .map(|text| text.to_std_string_escaped())
        })
        .transpose()
        .map_err(|error| js_error(error.to_string()))?
        .unwrap_or_default();
    dialog_open(this, false, context)?;
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(
                DomAttribute::new("data-megi-return-value", &return_value),
                true,
            );
            element
                .attributes
                .retain(|attr| attr.local_name != "data-megi-modal");
        }
    });
    let _ = crate::events::dispatch_event_type(this, "close", false, false, context);
    Ok(JsValue::undefined())
}

pub(crate) fn node_dialog_return_value_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| {
                element
                    .get_attribute("data-megi-return-value")
                    .map(str::to_owned)
            })
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}

pub(crate) fn node_value_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    let result = with_context_host(context, |host| {
        let tag = host
            .document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| element.local_name.clone())
            .ok_or_else(|| "node is not an element".to_string())?;
        if tag == "textarea" {
            if !host.initial_control_values.contains_key(&node_id) {
                let initial = host.document.text_content(node_id);
                host.initial_control_values.insert(node_id, Some(initial));
            }
            host.document
                .set_text_content(node_id, &value)
                .map_err(|error| error.to_string())?;
            return Ok::<(), String>(());
        }
        if tag == "select" {
            let options = option_ids(&host.document, node_id);
            for option_id in options {
                let opt_val = host
                    .document
                    .try_node(option_id)
                    .and_then(|n| n.as_element())
                    .map(|el| {
                        el.get_attribute("value")
                            .map(str::to_owned)
                            .unwrap_or_else(|| host.document.text_content(option_id))
                    })
                    .unwrap_or_default();
                let matches = opt_val == value;
                if !host.initial_control_checked.contains_key(&option_id) {
                    let was_selected = host
                        .document
                        .try_node(option_id)
                        .and_then(|n| n.as_element())
                        .is_some_and(|el| el.get_attribute("selected").is_some());
                    host.initial_control_checked.insert(option_id, was_selected);
                }
                if let Some(el) = host
                    .document
                    .try_node_mut(option_id)
                    .and_then(|n| n.as_element_mut())
                {
                    if matches {
                        el.set_attribute(DomAttribute::new("selected", ""), true);
                    } else {
                        el.remove_attribute("selected");
                    }
                }
            }
            return Ok(());
        }
        if !host.initial_control_values.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|node| node.as_element())
                .and_then(|el| el.get_attribute("value").map(str::to_owned));
            host.initial_control_values.insert(node_id, initial);
        }
        // Keep the content attribute untouched: `defaultValue` and form.reset() rely on it.
        host.idl_values.insert(node_id, value);
        Ok(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}
