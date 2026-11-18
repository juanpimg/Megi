use super::*;

pub(crate) fn node_focus(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| host.active_element = Some(node_id));
    Ok(JsValue::undefined())
}

pub(crate) fn node_blur(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    with_context_host(_context, |host| host.active_element = None);
    Ok(JsValue::undefined())
}

/// `Element.attachShadow({mode})` — creates an inert `DocumentFragment` as shadow root (#1541).
pub(crate) fn node_attach_shadow(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let existing =
        with_context_host(context, |host| host.shadow_roots.get(&node_id).copied()).flatten();
    if let Some(root) = existing {
        return wrap_node(context, root);
    }
    let root = with_context_host(context, |host| {
        let root = host.document.create_document_fragment();
        host.shadow_roots.insert(node_id, root);
        host.shadow_hosts.insert(root, node_id);
        Some(root)
    })
    .flatten()
    .ok_or_else(|| js_error("cannot attach shadow root"))?;
    wrap_node(context, root)
}

pub(crate) fn node_shadow_root_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let root =
        with_context_host(context, |host| host.shadow_roots.get(&node_id).copied()).flatten();
    match root {
        Some(root) => wrap_node(context, root),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn frame_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("frame method requires an object"))?;
    object
        .get(js_string!("__megi_frame"), context)
        .ok()
        .and_then(|v| v.as_number())
        .filter(|v| *v >= 0.0 && v.fract() == 0.0)
        .map(|v| v as u32)
        .ok_or_else(|| js_error("invalid frame document handle"))
}

pub(crate) fn frame_node(this: &JsValue, context: &mut Context) -> JsResult<(u32, NodeId)> {
    let handle = frame_handle(this, context)?;
    let node_handle = this
        .as_object()
        .and_then(|o| o.get(js_string!("__megi_frame_node"), context).ok())
        .and_then(|v| v.as_number())
        .ok_or_else(|| js_error("invalid frame node"))? as u32;
    let node = with_context_host(context, |host| host.frame_nodes.get(&node_handle).copied())
        .flatten()
        .ok_or_else(|| js_error("stale frame node"))?;
    Ok((handle, node))
}

pub(crate) fn frame_element_wrapper(
    context: &mut Context,
    handle: u32,
    node: NodeId,
) -> JsResult<JsValue> {
    let tag = with_context_host(context, |host| {
        host.frame_documents
            .get(&handle)
            .and_then(|d| d.try_node(node).and_then(|n| n.as_element()))
            .map(|e| e.local_name.clone())
    })
    .flatten()
    .unwrap_or_default();
    let node_handle = with_context_host(context, |host| {
        let h = host.frame_node_next_handle;
        host.frame_node_next_handle = host.frame_node_next_handle.wrapping_add(1);
        host.frame_nodes.insert(h, node);
        h
    })
    .ok_or_else(|| js_error("no host"))?;
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_frame"),
        JsValue::from(handle),
        Attribute::all(),
    );
    init.property(
        js_string!("__megi_frame_node"),
        JsValue::from(node_handle as f64),
        Attribute::all(),
    );
    init.property(
        js_string!("tagName"),
        JsValue::from(js_string!(tag.to_ascii_uppercase())),
        Attribute::all(),
    );
    init.function(
        native(frame_get_attribute as NativeFn),
        js_string!("getAttribute"),
        1,
    );
    let object = init.build();
    let getter = accessor_fn(context, frame_text_content);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(getter))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = object.define_property_or_throw(js_string!("textContent"), descriptor, context);
    Ok(object.into())
}

pub(crate) fn frame_get_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (handle, node) = frame_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_context_host(context, |host| {
        host.frame_documents
            .get(&handle)
            .and_then(|d| d.try_node(node).and_then(|n| n.as_element()))
            .and_then(|e| e.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    match value {
        Some(v) => Ok(JsValue::from(js_string!(v))),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn frame_text_content(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (handle, node) = frame_node(this, context)?;
    let text = with_context_host(context, |host| {
        host.frame_documents
            .get(&handle)
            .map(|d| d.text_content(node))
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

pub(crate) fn frame_get_element_by_id(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = frame_handle(this, context)?;
    let id = arg_string(args, 0, context)?;
    let node = with_context_host(context, |host| {
        host.frame_documents
            .get(&handle)
            .and_then(|d| d.get_element_by_id(&id))
    })
    .flatten();
    match node {
        Some(n) => frame_element_wrapper(context, handle, n),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn frame_query_selector_all(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let handle = frame_handle(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let nodes: Vec<NodeId> = with_context_host(context, |host| {
        host.frame_documents
            .get(&handle)
            .map(|d| d.get_elements_by_tag_name(&selector).into_iter().collect())
    })
    .flatten()
    .unwrap_or_default();
    let mut wrappers = Vec::with_capacity(nodes.len());
    for n in nodes {
        wrappers.push(frame_element_wrapper(context, handle, n)?);
    }
    Ok(JsArray::from_iter(wrappers, context).into())
}
