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

/// `Slotable.assignedSlot` (WHATWG DOM §4.9.2): the slot in the parent's shadow tree
/// whose name matches this node's `slot` attribute. Closed trees expose no slot.
pub(crate) fn node_assigned_slot_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let slot = with_context_host(context, |host| {
        let parent = host.document.try_node(node).and_then(|n| n.parent)?;
        let root = host.shadow_roots.get(&parent).copied()?;
        if host
            .shadow_modes
            .get(&parent)
            .is_some_and(|mode| mode == "closed")
        {
            return None;
        }
        let name = host
            .document
            .try_node(node)
            .and_then(|n| n.as_element())
            .and_then(|e| e.get_attribute("slot").map(str::to_owned))
            .unwrap_or_default();
        host.document.descendants(root).find(|id| {
            host.document
                .try_node(*id)
                .and_then(|n| n.as_element())
                .is_some_and(|e| {
                    e.local_name == "slot" && e.get_attribute("name").unwrap_or("") == name
                })
        })
    })
    .flatten();
    match slot {
        Some(id) => wrap_node(context, id),
        None => Ok(JsValue::null()),
    }
}

fn slot_name_of(host: &crate::host::Host, slot: megi_core::NodeId) -> String {
    host.document
        .try_node(slot)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("name").map(str::to_owned))
        .unwrap_or_default()
}

fn slot_root(
    host: &crate::host::Host,
    slot: megi_core::NodeId,
) -> Option<(megi_core::NodeId, megi_core::NodeId)> {
    let mut current = Some(slot);
    while let Some(id) = current {
        if let Some(host_node) = host.shadow_hosts.get(&id) {
            return Some((*host_node, id));
        }
        current = host.document.try_node(id).and_then(|node| node.parent);
    }
    None
}

fn is_slottable(host: &crate::host::Host, id: megi_core::NodeId) -> bool {
    host.document.try_node(id).is_some_and(|node| {
        matches!(
            node.kind,
            megi_core::NodeKind::Element(_) | megi_core::NodeKind::Text(_)
        )
    })
}

fn is_slot(host: &crate::host::Host, id: megi_core::NodeId) -> bool {
    host.document
        .try_node(id)
        .and_then(|node| node.as_element())
        .is_some_and(|element| element.local_name == "slot")
}

/// WHATWG DOM §4.9.3: only the first slot with a given name in a shadow tree receives
/// slottables; later slots with the same name stay empty.
fn assigned_nodes_of(host: &crate::host::Host, slot: megi_core::NodeId) -> Vec<megi_core::NodeId> {
    let Some((host_node, root)) = slot_root(host, slot) else {
        return Vec::new();
    };
    let name = slot_name_of(host, slot);
    let first = host
        .document
        .descendants(root)
        .find(|id| is_slot(host, *id) && slot_name_of(host, *id) == name);
    if first != Some(slot) {
        return Vec::new();
    }
    host.document
        .children(host_node)
        .filter(|id| {
            is_slottable(host, *id)
                && host
                    .document
                    .try_node(*id)
                    .and_then(|node| node.as_element())
                    .and_then(|element| element.get_attribute("slot").map(str::to_owned))
                    .unwrap_or_default()
                    == name
        })
        .collect()
}

fn flatten_node(
    host: &crate::host::Host,
    id: megi_core::NodeId,
    seen: &mut std::collections::HashSet<megi_core::NodeId>,
    out: &mut Vec<megi_core::NodeId>,
) {
    if !seen.insert(id) {
        return;
    }
    if !is_slot(host, id) {
        out.push(id);
        return;
    }
    let assigned = assigned_nodes_of(host, id);
    if assigned.is_empty() {
        for child in host.document.children(id) {
            flatten_node(host, child, seen, out);
        }
    } else {
        for child in assigned {
            flatten_node(host, child, seen, out);
        }
    }
}

/// `HTMLSlotElement.assignedNodes()` (WHATWG DOM §4.9.3): assigned slottables, or,
/// with `flatten`, their distributed nodes with fallback children for empty slots.
pub(crate) fn slot_assigned_nodes(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = this_node(this, context)?;
    let flatten = args
        .first()
        .and_then(JsValue::as_object)
        .and_then(|options| options.get(js_string!("flatten"), context).ok())
        .and_then(|value| value.as_boolean())
        .unwrap_or(false);
    let assigned = with_context_host(context, |host| {
        let direct = assigned_nodes_of(host, slot);
        if !flatten {
            return direct;
        }
        let seeds = if direct.is_empty() {
            // A detached slot has no fallback contents (WHATWG DOM §4.9.3).
            if slot_root(host, slot).is_none() {
                return Vec::new();
            }
            host.document
                .children(slot)
                .filter(|id| is_slottable(host, *id))
                .collect()
        } else {
            direct
        };
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in seeds {
            flatten_node(host, id, &mut seen, &mut out);
        }
        out
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
