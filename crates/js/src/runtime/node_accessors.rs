use super::*;

// ---------------------------------------------------------------------------
// Node accessors
// ---------------------------------------------------------------------------

pub(crate) fn node_tag_name(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = with_context_host(context, |host| {
        host.document.try_node(node_id).and_then(|node| {
            node.as_element()
                .map(|element| element.local_name.to_ascii_uppercase())
        })
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(name)))
}

pub(crate) fn node_id_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let id = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("id"))
            })
            .map(str::to_owned)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(id)))
}

pub(crate) fn node_id_set(
    this: &JsValue,
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
            element.set_attribute(DomAttribute::new("id", value), true);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_class_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let class = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("class"))
            })
            .map(str::to_owned)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(class)))
}

pub(crate) fn node_class_set(
    this: &JsValue,
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
            element.set_attribute(DomAttribute::new("class", value), true);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_text_content_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text =
        with_context_host(context, |host| host.document.text_content(node_id)).unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

pub(crate) fn node_text_content_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let text = arg_string(args, 0, context)?;
    let result = with_context_host(context, |host| {
        host.document.set_text_content(node_id, &text)
    });
    match result {
        Some(Ok(())) => {
            note_mutation(MutationKind::CharacterData, node_id, None, None, context);
            Ok(JsValue::undefined())
        }
        Some(Err(error)) => Err(js_error(error.to_string())),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_inner_html_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html = with_context_host(context, |host| {
        megi_html::serialize_fragment(&host.document, node_id)
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(html)))
}

pub(crate) fn node_inner_html_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html_text = arg_string(args, 0, context)?;
    let result = with_context_host(context, |host| {
        if !charge_document_parse(host, html_text.len(), "innerHTML assignment") {
            return Ok(());
        }
        let options = megi_html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..megi_html::ParseOptions::default()
        };
        megi_html::set_inner_html_with_options(&mut host.document, node_id, &html_text, &options)
            .map(|_| {
                host.record_mutation(MutationRecord {
                    kind: MutationKind::ChildList,
                    target: node_id,
                    name: None,
                    old_value: None,
                });
            })
            .map_err(|error| error.to_string())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_outer_html_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html = with_context_host(context, |host| {
        megi_html::serialize_outer_html(&host.document, node_id)
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(html)))
}

pub(crate) fn node_outer_html_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let html_text = arg_string(args, 0, context)?;
    let result = with_context_host(context, |host| {
        if !charge_document_parse(host, html_text.len(), "outerHTML assignment") {
            return Ok(());
        }
        let parent = host.document.try_node(node_id).and_then(|node| node.parent);
        let options = megi_html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..megi_html::ParseOptions::default()
        };
        megi_html::set_outer_html_with_options(&mut host.document, node_id, &html_text, &options)
            .map(|_| {
                if let Some(parent) = parent {
                    host.record_mutation(MutationRecord {
                        kind: MutationKind::ChildList,
                        target: parent,
                        name: None,
                        old_value: None,
                    });
                }
            })
            .map_err(|error| error.to_string())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_children(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let children = with_context_host(context, |host| {
        host.document
            .children(node_id)
            .filter(|child| {
                host.document
                    .try_node(*child)
                    .is_some_and(|node| node.as_element().is_some())
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    node_list(context, children)
}

pub(crate) fn node_parent_node(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let parent = with_context_host(context, |host| {
        host.document.try_node(node_id).and_then(|node| node.parent)
    });
    wrap_optional(context, parent.flatten())
}

pub(crate) fn node_first_child(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let child = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.first_child)
    });
    wrap_optional(context, child.flatten())
}

pub(crate) fn node_last_child(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let child = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.last_child)
    });
    wrap_optional(context, child.flatten())
}

pub(crate) fn node_previous_sibling(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let sibling = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.prev_sibling)
    });
    wrap_optional(context, sibling.flatten())
}

pub(crate) fn node_next_sibling(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let sibling = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.next_sibling)
    });
    wrap_optional(context, sibling.flatten())
}

pub(crate) fn node_style_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = this_handle(this, context)?;
    Ok(style_object(context, handle))
}

pub(crate) fn node_style_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // `element.style = "color: red"` replaces the inline style attribute.
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("style", value), true);
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_class_list_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = this_handle(this, context)?;
    Ok(class_list_object(context, handle))
}
