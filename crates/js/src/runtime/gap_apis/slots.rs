use super::*;

#[allow(dead_code)]
pub(crate) fn iframe_content_document(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let handle = with_context_host(context, |host| {
        if host
            .document
            .try_node(node)
            .and_then(|n| n.as_element())
            .is_some_and(|e| e.has_attribute("sandbox"))
        {
            return None;
        }
        host.iframe_frames.get(&node).copied()
    })
    .flatten();
    let Some(handle) = handle else {
        return Ok(JsValue::null());
    };
    let body_node = with_context_host(context, |host| {
        host.frame_documents.get(&handle).and_then(|d| d.body())
    })
    .flatten()
    .unwrap_or_else(|| NodeId::from_index(usize::MAX));
    let body = frame_element_wrapper(context, handle, body_node)?;
    let title = with_context_host(context, |host| {
        host.frame_documents.get(&handle).and_then(|d| {
            d.get_elements_by_tag_name("title")
                .first()
                .map(|n| d.text_content(*n))
        })
    })
    .flatten()
    .unwrap_or_default();
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_frame"),
        JsValue::from(handle),
        Attribute::all(),
    );
    init.function(
        native(frame_get_element_by_id as NativeFn),
        js_string!("getElementById"),
        1,
    );
    init.function(
        native(frame_query_selector_all as NativeFn),
        js_string!("querySelectorAll"),
        1,
    );
    init.property(
        js_string!("title"),
        JsValue::from(js_string!(title)),
        Attribute::all(),
    );
    init.property(js_string!("body"), body, Attribute::all());
    Ok(init.build().into())
}

pub(crate) fn slot_host(slot: NodeId, context: &mut Context) -> Option<NodeId> {
    with_context_host(context, |host| {
        let mut current = Some(slot);
        while let Some(id) = current {
            if let Some(host_node) = host.shadow_hosts.get(&id) {
                return Some(*host_node);
            }
            current = host.document.try_node(id).and_then(|node| node.parent);
        }
        None
    })
    .flatten()
}

/// `HTMLSlotElement.assignedNodes()` (#1541): returns the light-DOM children of the shadow host
/// assigned to this slot. Named slots take children with a matching `slot` attribute; the
/// default slot takes children without one. `flatten` is accepted for API compatibility.
pub(crate) fn slot_assigned_nodes(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = this_node(this, context)?;
    let Some(host_node) = slot_host(slot, context) else {
        return wrap_list(context, Vec::new());
    };
    let slot_name = with_context_host(context, |host| {
        host.document
            .try_node(slot)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute("name").map(str::to_owned))
    })
    .flatten()
    .unwrap_or_default();
    let _ = args; // `{ flatten: true }` does not change this subset's result.

    let assigned = with_context_host(context, |host| {
        let document = &host.document;
        let mut children = Vec::new();
        let mut child = document
            .try_node(host_node)
            .and_then(|node| node.first_child);
        while let Some(id) = child {
            let child_slot = document
                .try_node(id)
                .and_then(|node| node.as_element())
                .and_then(|element| element.get_attribute("slot").map(str::to_owned))
                .unwrap_or_default();
            let matches = if slot_name.is_empty() {
                child_slot.is_empty()
            } else {
                child_slot == slot_name
            };
            if matches {
                children.push(id);
            }
            child = document.try_node(id).and_then(|node| node.next_sibling);
        }
        children
    })
    .unwrap_or_default();
    wrap_list(context, assigned)
}

/// `HTMLSlotElement.assignedElements()` — like `assignedNodes()` but only element nodes.
pub(crate) fn slot_assigned_elements(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let nodes = match slot_assigned_nodes(this, args, context)? {
        value if value.as_object().is_some() => {
            let array = value.as_object().expect("object");
            let length = array
                .get(js_string!("length"), context)
                .ok()
                .and_then(|value| value.as_number())
                .unwrap_or(0.0) as u32;
            let mut elements = Vec::new();
            for index in 0..length {
                if let Ok(value) = array.get(index, context)
                    && let Some(object) = value.as_object()
                    && object
                        .get(js_string!("nodeType"), context)
                        .ok()
                        .and_then(|n| n.as_number())
                        == Some(1.0)
                {
                    elements.push(JsValue::from(object));
                }
            }
            return Ok(JsArray::from_iter(elements, context).into());
        }
        other => other,
    };
    Ok(nodes)
}

pub(crate) fn node_get_root_node(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let shadow = with_context_host(context, |host| {
        // A shadow host's own root is its shadow tree (per DOM §4.6 getRootNode).
        if let Some(root) = host.shadow_roots.get(&node_id) {
            return Some(*root);
        }
        // Descendants walk up to the shadow root that contains them.
        let mut current = Some(node_id);
        let mut found = None;
        while let Some(id) = current {
            if host.shadow_hosts.contains_key(&id) {
                found = Some(id);
                break;
            }
            current = host.document.try_node(id).and_then(|node| node.parent);
        }
        found
    })
    .flatten();
    match shadow {
        Some(root) => wrap_node(context, root),
        None => {
            let root = with_context_host(context, |host| host.document.root())
                .ok_or_else(|| js_error("no host"))?;
            wrap_node(context, root)
        }
    }
}

pub(crate) fn node_scroll_into_view(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    // Documented no-op: `js` has no layout/viewport provider for scrolling.
    Ok(JsValue::undefined())
}

pub(crate) fn document_active_element(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    match with_context_host(context, |host| host.active_element) {
        Some(Some(node_id)) => wrap_node(context, node_id),
        _ => Ok(JsValue::null()),
    }
}

pub(crate) fn document_normalize(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    with_context_host(_context, |host| {
        if let Some(root) = host.document.document_element() {
            let removed = host.document.normalize(root);
            if removed > 0 {
                host.record_mutation(MutationRecord {
                    kind: MutationKind::ChildList,
                    target: root,
                    name: None,
                    old_value: None,
                });
            }
        }
    });
    // `Document.normalize()` is inherited from `Node`: undefined (DOM §4.6).
    Ok(JsValue::undefined())
}

pub(crate) fn document_compare_document_position(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let first = argument_node(args, 0, context)?;
    let second = argument_node(args, 1, context)?;
    let bits = with_context_host(context, |host| {
        host.document.compare_document_position(first, second)
    })
    .unwrap_or(0);
    Ok(JsValue::from(bits as f64))
}
