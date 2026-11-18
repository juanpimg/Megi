use super::*;

pub(crate) fn validity_flags(node_id: NodeId, context: &mut Context) -> ValidityFlags {
    let computed = with_context_host(context, |host| {
        let document = &host.document;
        let element = document
            .try_node(node_id)
            .and_then(|node| node.as_element())?;
        Some(validity_flags_element(document, node_id, element))
    })
    .flatten();
    let Some((mut flags, pattern_check)) = computed else {
        return ValidityFlags::default();
    };
    if let Some((pattern, value)) = pattern_check
        && !test_pattern_in_context(&pattern, &value, context)
    {
        flags.pattern_mismatch = true;
    }
    flags
}

pub(crate) fn validate_control_and_fire_invalid(
    node_id: NodeId,
    wrapper: &JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let flags = validity_flags(node_id, context);
    if flags.is_valid() {
        Ok(true)
    } else {
        let _ = crate::events::dispatch_event_type(wrapper, "invalid", false, true, context)?;
        Ok(false)
    }
}

pub(crate) fn form_descendant_controls(
    document: &megi_core::Document,
    form_id: NodeId,
) -> Vec<NodeId> {
    document
        .descendants(form_id)
        .filter(|child| {
            document
                .try_node(*child)
                .and_then(|node| node.as_element())
                .is_some_and(|el| matches!(el.local_name.as_str(), "input" | "textarea" | "select"))
        })
        .collect()
}

pub(crate) fn validate_node_or_form(
    this: &JsValue,
    node_id: NodeId,
    context: &mut Context,
) -> JsResult<bool> {
    let is_form = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.local_name == "form")
    })
    .unwrap_or(false);
    if !is_form {
        return validate_control_and_fire_invalid(node_id, this, context);
    }
    let controls = with_context_host(context, |host| {
        form_descendant_controls(&host.document, node_id)
    })
    .unwrap_or_default();
    let mut all_valid = true;
    for control_id in controls {
        let wrapper = wrap_node(context, control_id)?;
        if !validate_control_and_fire_invalid(control_id, &wrapper, context)? {
            all_valid = false;
        }
    }
    Ok(all_valid)
}

pub(crate) fn node_check_validity(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let valid = validate_node_or_form(this, node_id, context)?;
    Ok(JsValue::from(valid))
}

pub(crate) fn node_report_validity(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let valid = validate_node_or_form(this, node_id, context)?;
    Ok(JsValue::from(valid))
}

pub(crate) fn node_set_custom_validity(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let message = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        let document = &mut host.document;
        let Some(node) = document.try_node_mut(node_id) else {
            return;
        };
        let Some(element) = node.as_element_mut() else {
            return;
        };
        if message.is_empty() {
            element
                .attributes
                .retain(|attr| attr.local_name != "data-megi-validity-error");
        } else {
            element.set_attribute(
                megi_core::Attribute::new("data-megi-validity-error", &message),
                true,
            );
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_validity_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let flags = validity_flags(node_id, context);
    let valid = flags.is_valid();
    let object = ObjectInitializer::new(context).build();
    for (name, value) in [
        ("valid", valid),
        ("valueMissing", flags.value_missing),
        ("customError", flags.custom_error),
        ("patternMismatch", flags.pattern_mismatch),
        ("typeMismatch", flags.type_mismatch),
        ("tooShort", flags.too_short),
        ("tooLong", flags.too_long),
        ("rangeUnderflow", flags.range_underflow),
        ("rangeOverflow", flags.range_overflow),
        ("stepMismatch", false),
        ("badInput", false),
    ] {
        let _ = object.set(js_string!(name), JsValue::from(value), false, context);
    }
    Ok(JsValue::from(object))
}

pub(crate) fn node_request_submit(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if !with_context_host(context, |host| host.allow_forms).unwrap_or(true) {
        return Ok(JsValue::from(false));
    }
    // `requestSubmit(submitter?)` runs constraint validation unless suppressed by `novalidate`
    // or submitter `formnovalidate`, and fires `submit` if valid (#1505, #1538).
    let node_id = this_node(this, context)?;
    let submitter_arg = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let submitter_id = if !submitter_arg.is_undefined() && !submitter_arg.is_null() {
        Some(this_node(&submitter_arg, context)?)
    } else {
        None
    };
    let skip_validation = with_context_host(context, |host| {
        let document = &host.document;
        let form_novalidate = document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.has_attribute("novalidate"));
        let submitter_novalidate = submitter_id.is_some_and(|id| {
            document
                .try_node(id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.has_attribute("formnovalidate"))
        });
        form_novalidate || submitter_novalidate
    })
    .unwrap_or(false);

    if !skip_validation && !validate_node_or_form(this, node_id, context)? {
        return Ok(JsValue::from(false));
    }
    let submitter = if submitter_id.is_some() {
        submitter_arg
    } else {
        JsValue::null()
    };
    let dispatched = crate::events::dispatch_submit_event(this, submitter, context)?;
    Ok(JsValue::from(dispatched))
}

pub(crate) fn node_submit(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if !with_context_host(context, |host| host.allow_forms).unwrap_or(true) {
        return Ok(JsValue::from(false));
    }
    // `form.submit()` bypasses constraint validation (#1505).
    let dispatched = crate::events::dispatch_submit_event(this, JsValue::null(), context)?;
    Ok(JsValue::from(dispatched))
}

pub(crate) fn node_reset(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let is_form = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.local_name == "form")
    })
    .unwrap_or(false);
    if !is_form {
        return Ok(JsValue::undefined());
    }
    let dispatched = crate::events::dispatch_event_type(this, "reset", true, true, context)?;
    if !dispatched {
        return Ok(JsValue::undefined());
    }
    with_context_host(context, |host| {
        let descendants: Vec<NodeId> = host.document.descendants(node_id).collect();
        for child_id in descendants {
            let tag = host
                .document
                .try_node(child_id)
                .and_then(|n| n.as_element())
                .map(|el| el.local_name.clone());
            let Some(tag) = tag else { continue };
            match tag.as_str() {
                "input" => {
                    if let Some(orig) = host.initial_control_values.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        match orig {
                            Some(v) => {
                                el.set_attribute(DomAttribute::new("value", v), true);
                            }
                            None => {
                                el.remove_attribute("value");
                            }
                        }
                    }
                    if let Some(orig_checked) = host.initial_control_checked.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        if orig_checked {
                            el.set_attribute(DomAttribute::new("checked", ""), true);
                        } else {
                            el.remove_attribute("checked");
                        }
                    }
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                "textarea" => {
                    if let Some(orig) = host.initial_control_values.remove(&child_id) {
                        let _ = host
                            .document
                            .set_text_content(child_id, &orig.unwrap_or_default());
                    }
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                "option" => {
                    if let Some(orig_selected) = host.initial_control_checked.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        if orig_selected {
                            el.set_attribute(DomAttribute::new("selected", ""), true);
                        } else {
                            el.remove_attribute("selected");
                        }
                    }
                }
                "select" => {
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                _ => {}
            }
        }
    });
    Ok(JsValue::undefined())
}
