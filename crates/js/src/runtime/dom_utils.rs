use super::*;

// ---------------------------------------------------------------------------
// DOM utilities: NodeList, matches/closest, insertAdjacent*
// ---------------------------------------------------------------------------

/// Deterministic accessibility snapshot (F7-A11Y, partial): a flat list of named
/// `{role, name}` nodes in document order. Platform/AT integration is out of scope here.
pub(crate) fn accessibility_tree_snapshot(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    const MAX_NODES: usize = 4096;
    let entries = with_context_host(context, |host| {
        let doc = &host.document;
        let mut entries = Vec::new();
        for id in doc.descendants(doc.root()) {
            if entries.len() >= MAX_NODES {
                break;
            }
            let Some(element) = doc.node(id).as_element() else {
                continue;
            };
            if element
                .get_attribute("aria-hidden")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            {
                continue;
            }
            let tag = element.local_name.as_str();
            let role = match tag {
                "a" => "link",
                "button" => "button",
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
                "img" => "img",
                "input" => match element.get_attribute("type") {
                    Some("checkbox") => "checkbox",
                    Some("radio") => "radio",
                    _ => "textbox",
                },
                "li" => "listitem",
                "nav" => "navigation",
                "table" => "table",
                _ => continue,
            };
            let name = element
                .get_attribute("aria-label")
                .or_else(|| element.get_attribute("alt"))
                .or_else(|| element.get_attribute("title"))
                .map(str::to_owned)
                .unwrap_or_else(|| doc.text_content(id));
            entries.push((role.to_owned(), name.trim().to_owned()));
        }
        entries
    })
    .unwrap_or_default();
    let mut objects = Vec::with_capacity(entries.len());
    for (role, name) in entries {
        let mut init = ObjectInitializer::new(context);
        init.property(js_string!("role"), js_string!(role), Attribute::all());
        init.property(js_string!("name"), js_string!(name), Attribute::all());
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
    let item = accessor_fn(context, node_list_item);
    let for_each = accessor_fn(context, node_list_for_each);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_node_list"), true, Attribute::all());
    for (index, wrapper) in wrappers.iter().enumerate() {
        init.property(
            js_string!(index.to_string()),
            wrapper.clone(),
            Attribute::all(),
        );
    }
    init.property(
        js_string!("length"),
        f64::from(wrappers.len() as u32),
        Attribute::all(),
    )
    .function(native(node_list_item), js_string!("item"), 1)
    .function(native(node_list_for_each), js_string!("forEach"), 1);
    let _ = (item, for_each);
    Ok(JsValue::from(init.build()))
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
    let length = object
        .get(js_string!("length"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .unwrap_or(0.0) as u64;
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
