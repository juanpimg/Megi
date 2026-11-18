use super::*;

// ---------------------------------------------------------------------------
// Round 27 DOM/JS gap APIs
// ---------------------------------------------------------------------------

pub(crate) fn node_inner_text_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text =
        with_context_host(context, |host| host.document.text_content(node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

pub(crate) fn camel_case_attribute(name: &str) -> Option<String> {
    let suffix = name.strip_prefix("data-")?;
    let mut out = String::new();
    let mut chars = suffix.chars().peekable();
    while let Some(ch) = chars.next() {
        // DOMStringMap only consumes a dash before an ASCII lowercase letter and
        // uppercases with ASCII rules; other dashes stay literal (#1451).
        if ch == '-' && chars.peek().is_some_and(|next| next.is_ascii_lowercase()) {
            let next = chars.next().expect("peeked lowercase ASCII letter");
            out.push(next.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

pub(crate) fn node_dataset_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let entries: Vec<(String, String)> = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| {
                element
                    .attributes
                    .iter()
                    .filter_map(|attribute| {
                        camel_case_attribute(&attribute.local_name)
                            .map(|key| (key, attribute.value.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
    .unwrap_or_default();
    let mut init = ObjectInitializer::new(context);
    for (key, value) in entries {
        init.property(
            js_string!(key),
            JsValue::from(js_string!(value)),
            Attribute::all(),
        );
    }
    Ok(JsValue::from(init.build()))
}

pub(crate) fn attribute_item(context: &mut Context, name: &str, value: &str) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("name"),
        JsValue::from(js_string!(name.to_string())),
        Attribute::all(),
    );
    init.property(
        js_string!("value"),
        JsValue::from(js_string!(value.to_string())),
        Attribute::all(),
    );
    Ok(JsValue::from(init.build()))
}

pub(crate) fn attributes_of(
    this: &JsValue,
    context: &mut Context,
) -> JsResult<(u32, Vec<(String, String)>)> {
    let node_id = this_node(this, context)?;
    let attributes = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| {
                element
                    .attributes
                    .iter()
                    .map(|attribute| (attribute.local_name.clone(), attribute.value.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
    .unwrap_or_default();
    let handle = with_context_host(context, |host| host.handle_for(node_id))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    Ok((handle, attributes))
}

pub(crate) fn node_attributes_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (handle, attributes) = attributes_of(this, context)?;
    let mut items = Vec::with_capacity(attributes.len());
    for (name, value) in &attributes {
        items.push(attribute_item(context, name, value)?);
    }
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_attr_node"),
        JsValue::from(f64::from(handle)),
        Attribute::all(),
    );
    init.property(
        js_string!("length"),
        JsValue::from(attributes.len() as f64),
        Attribute::all(),
    );
    for (index, item) in items.into_iter().enumerate() {
        init.property(js_string!(index.to_string()), item, Attribute::all());
    }
    init.function(native(attributes_item), js_string!("item"), 1);
    init.function(native(attributes_get_named), js_string!("getNamedItem"), 1);
    init.function(
        native(attributes_remove_named),
        js_string!("removeNamedItem"),
        1,
    );
    Ok(JsValue::from(init.build()))
}

pub(crate) fn attributes_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("attributes receiver"))?;
    object
        .get(js_string!("__megi_attr_node"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid NamedNodeMap"))
}

pub(crate) fn attributes_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let index = crate::events::webidl_uint32_index(args, context);
    let node_id = with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    let attribute = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.attributes.get(index))
            .map(|attribute| (attribute.local_name.clone(), attribute.value.clone()))
    })
    .flatten();
    match attribute {
        Some((name, value)) => attribute_item(context, &name, &value),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn attributes_get_named(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let name = arg_string(args, 0, context)?;
    let node_id = with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
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

pub(crate) fn attributes_remove_named(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = attributes_handle(this, context)?;
    let name = arg_string(args, 0, context)?;
    let node_id = with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    let old = with_context_host(context, |host| {
        host.document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .and_then(|element| element.remove_attribute(&name))
    })
    .flatten();
    note_mutation(
        MutationKind::Attributes,
        node_id,
        Some(name.clone()),
        old.clone(),
        context,
    );
    match old {
        Some(value) => attribute_item(context, &name, &value),
        None => Err(js_error("NotFoundError: attribute not present")),
    }
}

pub(crate) fn node_normalize(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        let removed = host.document.normalize(node_id);
        if removed > 0 {
            host.record_mutation(MutationRecord {
                kind: MutationKind::ChildList,
                target: node_id,
                name: None,
                old_value: None,
            });
        }
    });
    // DOM §4.6: `normalize()` returns undefined, not the number of merged nodes (#1373).
    Ok(JsValue::undefined())
}

pub(crate) fn node_compare_document_position(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let other = argument_node(args, 0, context)?;
    let bits = with_context_host(context, |host| {
        host.document.compare_document_position(node_id, other)
    })
    .unwrap_or(0);
    Ok(JsValue::from(bits as f64))
}
