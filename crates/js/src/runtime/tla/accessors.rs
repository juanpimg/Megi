use super::*;

pub(crate) fn node_node_type(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let kind = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .map(|node| match &node.kind {
                megi_core::NodeKind::Element(_) => 1.0,
                megi_core::NodeKind::Text(_) => 3.0,
                megi_core::NodeKind::Comment(_) => 8.0,
                megi_core::NodeKind::Document => 9.0,
                megi_core::NodeKind::DocumentFragment => 11.0,
                megi_core::NodeKind::Doctype { .. } => 10.0,
                megi_core::NodeKind::ProcessingInstruction { .. } => 7.0,
            })
    })
    .flatten()
    .unwrap_or(0.0);
    Ok(JsValue::from(kind))
}

pub(crate) fn string_attribute_get(
    this: &JsValue,
    name: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(name).map(str::to_owned))
    })
    .flatten();
    Ok(value.map_or_else(JsValue::null, |value| JsValue::from(js_string!(value))))
}

pub(crate) fn string_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(megi_core::Attribute::new(name.to_string(), value), true);
        }
        if matches!(name, "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_title_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "title", c)
}
pub(crate) fn node_title_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "title", a, c)
}
pub(crate) fn node_lang_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "lang", c)
}
pub(crate) fn node_lang_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "lang", a, c)
}
pub(crate) fn node_href_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "href", c)
}
pub(crate) fn node_href_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "href", a, c)
}
pub(crate) fn node_src_get(this: &JsValue, _a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_get(this, "src", c)
}
pub(crate) fn node_src_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "src", a, c)
}

pub(crate) fn node_action_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let value = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("action").map(str::to_owned))
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}
pub(crate) fn node_action_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "action", a, c)
}

pub(crate) fn node_method_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let method = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("method"))
            .map(|value| {
                if value.eq_ignore_ascii_case("post") {
                    "post"
                } else if value.eq_ignore_ascii_case("dialog") {
                    "dialog"
                } else {
                    "get"
                }
            })
            .unwrap_or("get")
    })
    .unwrap_or("get");
    Ok(JsValue::from(js_string!(method)))
}
pub(crate) fn node_method_set(this: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    string_attribute_set(this, "method", a, c)
}

pub(crate) fn node_enctype_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, c)?;
    let enctype = with_context_host(c, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("enctype"))
            .map(|value| {
                if value.eq_ignore_ascii_case("multipart/form-data") {
                    "multipart/form-data"
                } else if value.eq_ignore_ascii_case("text/plain") {
                    "text/plain"
                } else {
                    "application/x-www-form-urlencoded"
                }
            })
            .unwrap_or("application/x-www-form-urlencoded")
    })
    .unwrap_or("application/x-www-form-urlencoded");
    Ok(JsValue::from(js_string!(enctype)))
}
pub(crate) fn node_enctype_set(
    this: &JsValue,
    a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    string_attribute_set(this, "enctype", a, c)
}

pub(crate) fn node_no_validate_get(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "novalidate", c)
}
pub(crate) fn node_no_validate_set(
    this: &JsValue,
    a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "novalidate", a, c)
}

pub(crate) fn node_elements_get(
    this: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let controls: Vec<NodeId> = with_context_host(context, |host| {
        let document = &host.document;
        document
            .descendants(node_id)
            .filter(|child| {
                document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|el| match el.local_name.as_str() {
                        "button" | "fieldset" | "select" | "textarea" => true,
                        "input" => !el
                            .get_attribute("type")
                            .is_some_and(|t| t.eq_ignore_ascii_case("image")),
                        _ => false,
                    })
            })
            .collect()
    })
    .unwrap_or_default();
    node_list(context, controls)
}

pub(crate) fn option_ids(document: &megi_core::Document, select: NodeId) -> Vec<NodeId> {
    document
        .descendants(select)
        .filter(|id| {
            document
                .try_node(*id)
                .and_then(|node| node.as_element())
                .is_some_and(|element| element.local_name == "option")
        })
        .collect()
}

pub(crate) fn node_selected_index_get(
    this: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = with_context_host(context, |host| {
        option_ids(&host.document, node_id)
            .iter()
            .position(|id| {
                host.document
                    .try_node(*id)
                    .and_then(|node| node.as_element())
                    .is_some_and(|element| element.get_attribute("selected").is_some())
            })
            .map(|value| value as f64)
            .unwrap_or(-1.0)
    })
    .unwrap_or(-1.0);
    Ok(JsValue::from(index))
}

pub(crate) fn node_selected_index_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let index = args
        .first()
        .and_then(|value| value.to_number(context).ok())
        .unwrap_or(-1.0) as i64;
    with_context_host(context, |host| {
        let options = option_ids(&host.document, node_id);
        for (position, option) in options.iter().enumerate() {
            if !host.initial_control_checked.contains_key(option) {
                let was_selected = host
                    .document
                    .try_node(*option)
                    .and_then(|n| n.as_element())
                    .is_some_and(|el| el.get_attribute("selected").is_some());
                host.initial_control_checked.insert(*option, was_selected);
            }
            if let Some(element) = host
                .document
                .try_node_mut(*option)
                .and_then(|node| node.as_element_mut())
            {
                if position as i64 == index {
                    element
                        .set_attribute(megi_core::Attribute::new("selected", String::new()), true);
                } else {
                    element
                        .attributes
                        .retain(|attribute| attribute.local_name != "selected");
                }
            }
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn class_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    let index = crate::events::webidl_uint32_index(args, context);
    Ok(classes.get(index).map_or_else(JsValue::null, |value| {
        JsValue::from(js_string!(value.clone()))
    }))
}

pub(crate) fn node_get_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    match value {
        Some(value) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn node_remove_attribute_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let item = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("removeAttributeNode expects an Attr"))?;
    let raw_name = item
        .get(js_string!("name"), context)
        .map_err(|error| js_error(error.to_string()))?
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .unwrap_or_default();
    let name = attribute_name(node_id, &raw_name, context);
    let old = with_context_host(context, |host| {
        host.document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .and_then(|element| element.remove_attribute(&name))
    })
    .flatten();
    match old {
        Some(value) => {
            // The other attribute-removal paths record a mutation; removeAttributeNode
            // must too, otherwise observers watching `attributes` miss the removal (#1137).
            note_mutation(
                MutationKind::Attributes,
                node_id,
                Some(name.clone()),
                Some(value.clone()),
                context,
            );
            attribute_item(context, &name, &value)
        }
        None => Ok(JsValue::null()),
    }
}
