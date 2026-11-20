use super::*;

// ---------------------------------------------------------------------------
// DOM utilities: NodeList, matches/closest, insertAdjacent*
// ---------------------------------------------------------------------------

/// #1851: hard cap on native `forEach` iterations so a tampered length cannot
/// turn a snapshot walk into an unbudgeted billion-call loop.
const MAX_NODE_LIST_ITERATIONS: u64 = 1_000_000;

/// Deterministic accessibility snapshot (F7-A11Y, partial): a flat list of named
/// `{role, name}` nodes in document order. Platform/AT integration is out of scope here.
pub(crate) fn accessibility_tree_snapshot(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // F7-A11Y: expose the structured tree (role, name, states, parent/children) so a
    // platform backend can consume the same snapshot that is used by tests.
    let tree = with_context_host(context, |host| {
        build_accessibility_tree(&host.document, host.active_element)
    })
    .unwrap_or_default();
    let mut objects = Vec::with_capacity(tree.nodes.len());
    for node in &tree.nodes {
        // Build arrays before `init` borrows `context` mutably (#borrow rules).
        let states = JsValue::from(JsArray::from_iter(
            node.states
                .iter()
                .cloned()
                .map(|state| JsValue::from(js_string!(state))),
            context,
        ));
        let children = JsValue::from(JsArray::from_iter(
            node.children
                .iter()
                .map(|index| JsValue::from(*index as u32)),
            context,
        ));
        let mut init = ObjectInitializer::new(context);
        init.property(
            js_string!("role"),
            js_string!(node.role.clone()),
            Attribute::all(),
        );
        init.property(
            js_string!("name"),
            js_string!(node.name.clone()),
            Attribute::all(),
        );
        init.property(
            js_string!("description"),
            js_string!(node.description.clone()),
            Attribute::all(),
        );
        if let Some(level) = node.level {
            init.property(js_string!("level"), JsValue::from(level), Attribute::all());
        }
        if let Some(value) = &node.value {
            init.property(
                js_string!("value"),
                js_string!(value.clone()),
                Attribute::all(),
            );
        }
        init.property(js_string!("states"), states, Attribute::all());
        init.property(js_string!("children"), children, Attribute::all());
        if let Some(parent) = node.parent {
            init.property(
                js_string!("parent"),
                JsValue::from(parent as u32),
                Attribute::all(),
            );
        }
        objects.push(JsValue::from(init.build()));
    }
    Ok(JsValue::from(JsArray::from_iter(objects, context)))
}

/// Snapshot `NodeList` object (`length`, numeric indices, `item`, `forEach`).
pub(crate) fn node_list(context: &mut Context, nodes: Vec<NodeId>) -> JsResult<JsValue> {
    let mut wrappers = Vec::with_capacity(nodes.len());
    for node in nodes {
        wrappers.push(wrap_node(context, node)?);
    }
    let snapshot_size = wrappers.len();
    // #1937: an Array is natively iterable (`Symbol.iterator`), so `for...of` works on
    // HTMLCollections just like on NodeLists. Keep the DOM collection helpers as properties.
    let array = JsValue::from(JsArray::from_iter(wrappers, context));
    let object = array.as_object().expect("array object");
    let _ = object.set(
        js_string!("__megi_node_list"),
        JsValue::from(true),
        false,
        context,
    );
    // #1851: Web IDL §3.7.8 declares `length` readonly. The backing Array's
    // `length` is writable, which let a script inflate `forEach` into a billion
    // native (unbudgeted) callback calls. Make it non-writable and keep the real
    // snapshot length in a private field that `forEach` reads instead.
    let snapshot_len = JsValue::from(snapshot_size as u64);
    let length_descriptor = PropertyDescriptor::builder()
        .value(snapshot_len.clone())
        .writable(false)
        .enumerable(false)
        .configurable(false)
        .build();
    let _ = object.define_property_or_throw(js_string!("length"), length_descriptor, context);
    let snapshot_descriptor = PropertyDescriptor::builder()
        .value(snapshot_len)
        .writable(false)
        .enumerable(false)
        .configurable(false)
        .build();
    let _ = object.define_property_or_throw(
        js_string!("__megi_node_list_length"),
        snapshot_descriptor,
        context,
    );
    let _ = object.set(
        js_string!("item"),
        JsValue::from(accessor_fn(context, node_list_item)),
        false,
        context,
    );
    let _ = object.set(
        js_string!("forEach"),
        JsValue::from(accessor_fn(context, node_list_for_each)),
        false,
        context,
    );
    Ok(JsValue::from(object.clone()))
}

pub(crate) fn node_list_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = crate::events::webidl_uint32_index(args, context);
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid NodeList receiver"))?;
    let value = object
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))?;
    if value.is_undefined() {
        Ok(JsValue::null())
    } else {
        Ok(value)
    }
}

pub(crate) fn node_list_for_each(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let callback = args
        .first()
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("forEach expects a function"))?;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid NodeList receiver"))?;
    // #1851: use the real snapshot length, never the public `length` property,
    // which scripts may have tampered with. Fall back for foreign node-lists.
    let length = object
        .get(js_string!("__megi_node_list_length"), context)
        .ok()
        .and_then(|value| value.as_number())
        .or_else(|| {
            object
                .get(js_string!("length"), context)
                .ok()
                .and_then(|value| value.as_number())
        })
        .unwrap_or(0.0) as u64;
    // Guard against absurd lengths even if a foreign list exposes a bogus value.
    let length = length.min(MAX_NODE_LIST_ITERATIONS);
    for index in 0..length {
        let item = object
            .get(js_string!(index.to_string()), context)
            .map_err(|error| js_error(error.to_string()))?;
        callback
            .call(
                &JsValue::undefined(),
                &[item, JsValue::from(index as f64)],
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(JsValue::undefined())
}

pub(crate) fn node_matches(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let matched = with_context_host(context, |host| {
        megi_css::query::matches(&host.document, node, &selector)
    })
    .unwrap_or(false);
    Ok(JsValue::from(matched))
}

pub(crate) fn node_closest(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let mut current = Some(node);
    while let Some(candidate) = current {
        if with_context_host(context, |host| {
            megi_css::query::matches(&host.document, candidate, &selector)
        })
        .unwrap_or(false)
        {
            return wrap_node(context, candidate);
        }
        current = with_context_host(context, |host| {
            host.document
                .try_node(candidate)
                .and_then(|node| node.parent)
        })
        .flatten();
    }
    Ok(JsValue::null())
}

pub(crate) fn node_get_elements_by_tag_name(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    // #1875: Element.getElementsByTagName scopes the collection to descendants of `this`.
    let node = this_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let nodes = with_context_host(context, |host| {
        let doc = &host.document;
        doc.descendants(node)
            .filter(|id| {
                doc.node(*id).as_element().is_some_and(|element| {
                    name == "*" || element.local_name.eq_ignore_ascii_case(&name)
                })
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    wrap_list(context, nodes)
}

pub(crate) fn node_query_selector(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let found = with_context_host(context, |host| {
        megi_css::query::query_selector_from(&host.document, node, &selector)
    });
    wrap_optional(context, found.flatten())
}

pub(crate) fn node_query_selector_all(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let selector = arg_string(args, 0, context)?;
    let nodes = with_context_host(context, |host| {
        megi_css::query::query_selector_all_from(&host.document, node, &selector)
    })
    .unwrap_or_default();
    wrap_list(context, nodes)
}

pub(crate) fn insert_relative(
    host: &mut crate::host::Host,
    target: NodeId,
    node: NodeId,
    position: AdjacentPosition,
) -> Result<(), String> {
    let container = match position {
        AdjacentPosition::BeforeBegin => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                host.document
                    .insert_before_child(parent, node, Some(target))
                    .map_err(|err| err.to_string())?;
            }
            parent
        }
        AdjacentPosition::AfterBegin => {
            let first = host
                .document
                .try_node(target)
                .and_then(|node| node.first_child);
            host.document
                .insert_before_child(target, node, first)
                .map_err(|err| err.to_string())?;
            Some(target)
        }
        AdjacentPosition::BeforeEnd => {
            host.document
                .insert_before_child(target, node, None)
                .map_err(|err| err.to_string())?;
            Some(target)
        }
        AdjacentPosition::AfterEnd => {
            let parent = host.document.try_node(target).and_then(|node| node.parent);
            if let Some(parent) = parent {
                let next = host
                    .document
                    .try_node(target)
                    .and_then(|node| node.next_sibling);
                host.document
                    .insert_before_child(parent, node, next)
                    .map_err(|err| err.to_string())?;
            }
            parent
        }
    };
    if let Some(container) = container {
        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target: container,
            name: None,
            old_value: None,
        });
    }
    Ok(())
}

/// Validates the `position` argument of `insertAdjacent*`, throwing `SyntaxError` for
/// anything outside the four defined values (#653).
pub(crate) fn adjacent_position(
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<AdjacentPosition> {
    let raw = arg_string(args, 0, context)?;
    let lowered = raw.to_ascii_lowercase();
    AdjacentPosition::parse(&lowered).ok_or_else(|| {
        syntax_error(format!(
            "insertAdjacent*: unknown position `{raw}` (expected beforebegin, afterbegin, beforeend or afterend)"
        ))
    })
}

pub(crate) fn node_insert_adjacent_html(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let is_element = with_context_host(context, |host| {
        host.document
            .try_node(target)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element {
        return Err(js_error("insertAdjacentHTML is only supported on elements"));
    }
    let position = adjacent_position(args, context)?;
    let html_text = arg_string(args, 1, context)?;
    let result = with_context_host(context, |host| {
        if !charge_document_parse(host, html_text.len(), "insertAdjacentHTML") {
            return Ok(());
        }
        let temp = host
            .document
            .create_element(Namespace::Html, "div", Vec::new());
        let options = megi_html::ParseOptions {
            max_nodes: host.max_document_write_nodes,
            ..megi_html::ParseOptions::default()
        };
        let parsed =
            megi_html::set_inner_html_with_options(&mut host.document, temp, &html_text, &options);
        if let Err(err) = parsed {
            host.document.remove_subtree(temp);
            return Err(err.to_string());
        }
        let children: Vec<NodeId> = host.document.children(temp).collect();
        let mut insert_err = None;
        for child in children {
            if let Err(err) = insert_relative(host, target, child, position) {
                insert_err = Some(err);
                break;
            }
        }
        host.document.remove_subtree(temp);
        match insert_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    })
    .unwrap_or(Ok(()));
    result.map_err(js_error)?;
    Ok(JsValue::undefined())
}

pub(crate) fn node_insert_adjacent_text(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let is_element = with_context_host(context, |host| {
        host.document
            .try_node(target)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element {
        return Err(js_error("insertAdjacentText is only supported on elements"));
    }
    let position = adjacent_position(args, context)?;
    let text = arg_string(args, 1, context)?;
    let result = with_context_host(context, |host| {
        let node = host.document.create_text_node(&text);
        if let Err(err) = insert_relative(host, target, node, position) {
            host.document.remove_subtree(node);
            return Err(err);
        }
        Ok(())
    })
    .unwrap_or(Ok(()));
    result.map_err(js_error)?;
    Ok(JsValue::undefined())
}

// #1942: insertAdjacentElement — insert an existing element at an adjacent position.
pub(crate) fn node_insert_adjacent_element(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = this_node(this, context)?;
    let is_element = with_context_host(context, |host| {
        host.document
            .try_node(target)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element {
        return Err(js_error(
            "insertAdjacentElement is only supported on elements",
        ));
    }
    let position = adjacent_position(args, context)?;
    let element_node = this_node(&args[1], context)?;
    let is_element_node = with_context_host(context, |host| {
        host.document
            .try_node(element_node)
            .is_some_and(|node| node.as_element().is_some())
    })
    .unwrap_or(false);
    if !is_element_node {
        return Err(js_error("insertAdjacentElement requires an element"));
    }
    let result = with_context_host(context, |host| {
        insert_relative(host, target, element_node, position)
    })
    .unwrap_or(Ok(()));
    result.map_err(js_error)?;
    wrap_node(context, element_node)
}
