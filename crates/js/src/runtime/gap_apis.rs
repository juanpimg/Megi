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

/// Resolves a canvas 2D context receiver to its `<canvas>` node id.
pub(crate) fn canvas_context_node(this: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("canvas method called with a non-object receiver"))?;
    let value = object
        .get(js_string!("__megi_canvas_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    let handle = value
        .as_number()
        .filter(|n| *n >= 0.0 && *n <= u32::MAX as f64 && n.fract() == 0.0)
        .map(|n| n as u32)
        .ok_or_else(|| js_error("invalid canvas context"))?;
    with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

/// `HTMLCanvasElement.getContext('2d')` (#1545): returns a context object bound to the canvas
/// node. Repeated calls return a fresh wrapper over the same underlying surface.
pub(crate) fn canvas_get_context(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let kind = arg_string(args, 0, context)?;
    if kind != "2d" {
        return Ok(JsValue::null());
    }
    let node = this_node(this, context)?;
    let (width, height) = with_context_host(context, |host| {
        let document = &host.document;
        let element = document.try_node(node).and_then(|n| n.as_element());
        let dim = |name: &str, default: u32| -> u32 {
            element
                .and_then(|e| e.get_attribute(name))
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(default)
                .min(crate::canvas::MAX_CANVAS_DIM)
        };
        (dim("width", 300), dim("height", 150))
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    let surface_ok = with_context_host(context, |host| {
        host.canvas_store.surface_for(node, width, height).is_some()
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;
    if !surface_ok {
        return Err(js_error("canvas surface exceeds size cap"));
    }
    let handle = with_context_host(context, |host| host.handle_for(node))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;

    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_canvas_node"),
        JsValue::from(handle),
        Attribute::all(),
    );
    for (name, function) in [
        ("fillRect", canvas_fill_rect as NativeFn),
        ("clearRect", canvas_clear_rect as NativeFn),
        ("strokeRect", canvas_stroke_rect as NativeFn),
        ("getImageData", canvas_get_image_data as NativeFn),
        ("putImageData", canvas_put_image_data as NativeFn),
    ] {
        init.function(native(function), js_string!(name), 4);
    }
    let context_object = init.build();
    // fillStyle/strokeStyle accessors (defined after build via the descriptor API).
    let get_fill = accessor_fn(context, canvas_fill_style_get);
    let set_fill = accessor_fn(context, canvas_fill_style_set);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(get_fill))
        .set(JsValue::from(set_fill))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = context_object.define_property_or_throw(js_string!("fillStyle"), descriptor, context);
    let get_stroke = accessor_fn(context, canvas_stroke_style_get);
    let set_stroke = accessor_fn(context, canvas_stroke_style_set);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(get_stroke))
        .set(JsValue::from(set_stroke))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = context_object.define_property_or_throw(js_string!("strokeStyle"), descriptor, context);
    Ok(context_object.into())
}

pub(crate) fn canvas_fill_rect(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let rect = canvas_rect_args(args, context)?;
    with_context_host(context, |host| {
        host.canvas_store
            .with_surface_mut(node, |s| s.fill_rect(rect.0, rect.1, rect.2, rect.3))
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_clear_rect(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let rect = canvas_rect_args(args, context)?;
    with_context_host(context, |host| {
        host.canvas_store
            .with_surface_mut(node, |s| s.clear_rect(rect.0, rect.1, rect.2, rect.3))
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_stroke_rect(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let rect = canvas_rect_args(args, context)?;
    with_context_host(context, |host| {
        host.canvas_store
            .with_surface_mut(node, |s| s.stroke_rect(rect.0, rect.1, rect.2, rect.3))
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_rect_args(
    args: &[JsValue],
    _context: &mut Context,
) -> JsResult<(f64, f64, f64, f64)> {
    let num = |index: usize| -> JsResult<f64> {
        args.get(index)
            .and_then(|v| v.as_number())
            .ok_or_else(|| js_error("canvas rect expects four finite numbers"))
    };
    Ok((num(0)?, num(1)?, num(2)?, num(3)?))
}

pub(crate) fn canvas_fill_style_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let css = with_context_host(context, |host| {
        host.canvas_store
            .get(node)
            .map(|s| crate::canvas::rgba_to_css(s.fill_style))
    })
    .flatten()
    .unwrap_or_else(|| "#000000".to_owned());
    Ok(JsValue::from(js_string!(css)))
}

pub(crate) fn canvas_fill_style_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    if let Some(color) = crate::canvas::Rgba::parse(&value) {
        with_context_host(context, |host| {
            host.canvas_store
                .with_surface_mut(node, |s| s.fill_style = color)
        });
    }
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_stroke_style_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let css = with_context_host(context, |host| {
        host.canvas_store
            .get(node)
            .map(|s| crate::canvas::rgba_to_css(s.stroke_style))
    })
    .flatten()
    .unwrap_or_else(|| "#000000".to_owned());
    Ok(JsValue::from(js_string!(css)))
}

pub(crate) fn canvas_stroke_style_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    if let Some(color) = crate::canvas::Rgba::parse(&value) {
        with_context_host(context, |host| {
            host.canvas_store
                .with_surface_mut(node, |s| s.stroke_style = color)
        });
    }
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_get_image_data(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = args.first().and_then(|v| v.as_number()).unwrap_or(0.0) as i64;
    let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as i64;
    let w = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
    let h = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as u32;
    let data = with_context_host(context, |host| {
        host.canvas_store
            .get(node)
            .and_then(|s| s.get_image_data(x, y, w, h))
    })
    .flatten();
    let Some(data) = data else {
        return Ok(JsValue::null());
    };
    // Return an object { width, height, data: Uint8ClampedArray }.
    let array =
        boa_engine::object::builtins::JsUint8ClampedArray::from_iter(data.iter().copied(), context)
            .map_err(|error| js_error(error.to_string()))?;
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("width"), JsValue::from(w), Attribute::all());
    init.property(js_string!("height"), JsValue::from(h), Attribute::all());
    init.property(js_string!("data"), JsValue::from(array), Attribute::all());
    Ok(init.build().into())
}

pub(crate) fn canvas_put_image_data(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let image = args
        .first()
        .and_then(|v| v.as_object())
        .ok_or_else(|| js_error("putImageData expects ImageData"))?;
    let dx = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as i64;
    let dy = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as i64;
    let w = image
        .get(js_string!("width"), context)
        .ok()
        .and_then(|v| v.as_number())
        .unwrap_or(0.0) as u32;
    let h = image
        .get(js_string!("height"), context)
        .ok()
        .and_then(|v| v.as_number())
        .unwrap_or(0.0) as u32;
    let data_obj = image
        .get(js_string!("data"), context)
        .ok()
        .and_then(|v| v.as_object());
    let bytes: Vec<u8> = data_obj
        .as_ref()
        .and_then(|obj| {
            obj.get(js_string!("length"), context)
                .ok()
                .and_then(|v| v.as_number())
        })
        .map(|len| {
            let obj = data_obj.as_ref().expect("obj");
            (0..len as u32)
                .filter_map(|i| {
                    obj.get(js_string!(i.to_string()), context)
                        .ok()
                        .and_then(|v| v.as_number())
                })
                .map(|n| n.clamp(0.0, 255.0) as u8)
                .collect()
        })
        .unwrap_or_default();
    with_context_host(context, |host| {
        host.canvas_store.with_surface_mut(node, |s| {
            let _ = s.put_image_data(dx, dy, w, h, &bytes);
        })
    });
    Ok(JsValue::undefined())
}

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

/// Installs small standard-library gap APIs (URL/SearchParams, base64, structuredClone,
/// queueMicrotask, requestIdleCallback, MouseEvent/PointerEvent and Node constants).
pub(crate) fn install_gap_apis(context: &mut Context) -> Result<(), JsError> {
    let global = context.global_object();
    for (name, getter) in [
        ("innerWidth", window_inner_width as NativeFn),
        ("innerHeight", window_inner_height),
        ("devicePixelRatio", window_device_pixel_ratio),
    ] {
        let get = accessor_fn(context, getter);
        let descriptor = PropertyDescriptor::builder()
            .get(JsValue::from(get))
            .enumerable(true)
            .configurable(true)
            .build();
        global
            .define_property_or_throw(js_string!(name), descriptor, context)
            .map_err(|error| JsError::Internal(error.to_string()))?;
    }
    const PRELUDE: &str = r####"
(function () {
  function define(name, value) { window[name] = value; }
  function urlNormalizePath(path) {
    var segments = String(path).split("/");
    var out = [];
    for (var i = 0; i < segments.length; i++) {
      var segment = segments[i];
      var last = i === segments.length - 1;
      if (segment === ".") { if (last) { out.push(""); } continue; }
      if (segment === "..") {
        if (out.length > 1 || (out.length === 1 && out[0] !== "")) { out.pop(); }
        if (last) { out.push(""); }
        continue;
      }
      out.push(segment);
    }
    var result = out.join("/");
    if (result === "") { return "/"; }
    if (result.charAt(0) !== "/") { result = "/" + result; }
    return result;
  }
  function urlSplitAuthority(authority) {
    var out = { userinfo: "", host: "", hostname: "", port: "" };
    if (!authority) { return out; }
    var hostport = authority;
    var at = authority.lastIndexOf("@");
    if (at >= 0) { out.userinfo = authority.substring(0, at); hostport = authority.substring(at + 1); }
    var hostname = hostport;
    var port = "";
    if (hostport.charAt(0) === "[") {
      var close = hostport.indexOf("]");
      if (close > 0) {
        hostname = hostport.substring(0, close + 1);
        var rest = hostport.substring(close + 1);
        if (rest.charAt(0) === ":") { port = rest.substring(1); }
      }
    } else {
      var colon = hostport.indexOf(":");
      if (colon >= 0) { hostname = hostport.substring(0, colon); port = hostport.substring(colon + 1); }
    }
    out.hostname = hostname;
    out.port = port;
    out.host = port ? hostname + ":" + port : hostname;
    return out;
  }
  function urlBuild(protocol, userinfo, hostname, port, path, search, hash) {
    var hasAuthority = hostname !== "";
    var pathname = path === "" ? (hasAuthority ? "/" : "") : path;
    if (hasAuthority || pathname.charAt(0) === "/") { pathname = urlNormalizePath(pathname); }
    var searchText = search || "";
    if (searchText !== "" && searchText.charAt(0) !== "?") { searchText = "?" + searchText; }
    var hashText = hash || "";
    if (hashText !== "" && hashText.charAt(0) !== "#") { hashText = "#" + hashText; }
    var authority = hasAuthority
      ? "//" + (userinfo ? userinfo + "@" : "") + hostname + (port ? ":" + port : "")
      : "";
    var host = hasAuthority ? (port ? hostname + ":" + port : hostname) : "";
    var credentials = userinfo ? userinfo.split(":") : [];
    var result = {
      href: protocol + authority + pathname + searchText + hashText,
      protocol: protocol,
      username: credentials.length > 0 ? credentials[0] : "",
      password: credentials.length > 1 ? credentials.slice(1).join(":") : "",
      host: host,
      hostname: hostname,
      port: port,
      pathname: pathname,
      search: searchText,
      hash: hashText,
      origin: hasAuthority ? protocol + "//" + host : "null"
    };
    result.searchParams = null;
    return result;
  }
  function urlParse(input, base) {
    var value = String(input);
    var baseString = base === undefined || base === null ? "" : String(base);
    var scheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(value);
    var protocolRelative = false;
    if (!scheme && value.substring(0, 2) === "//") {
      // Protocol-relative: inherit the base scheme but not its authority.
      var baseScheme = /^([A-Za-z][A-Za-z0-9+.-]*:)/.exec(baseString);
      if (!baseScheme) { throw new TypeError("Invalid URL: " + value); }
      scheme = baseScheme;
      protocolRelative = true;
    }
    if (!scheme) {
      if (!baseString) { throw new TypeError("Invalid URL: " + value); }
      var resolvedBase = urlParse(baseString, null);
      var path = resolvedBase.pathname;
      var search = resolvedBase.search;
      var hash = "";
      var local = value;
      var hashIndex = local.indexOf("#");
      if (hashIndex >= 0) { hash = local.substring(hashIndex); local = local.substring(0, hashIndex); }
      var localQuery = "";
      var queryIndex = local.indexOf("?");
      if (queryIndex >= 0) { localQuery = local.substring(queryIndex); local = local.substring(0, queryIndex); }
      if (local === "") {
        if (queryIndex >= 0) { search = localQuery; } else if (hashIndex < 0) { search = resolvedBase.search; }
      } else if (local.charAt(0) === "/") {
        path = local;
        search = localQuery;
      } else {
        var slash = path.lastIndexOf("/");
        path = (slash >= 0 ? path.substring(0, slash + 1) : "/") + local;
        search = localQuery;
      }
      var baseUserinfo = resolvedBase.username
        ? (resolvedBase.password ? resolvedBase.username + ":" + resolvedBase.password : resolvedBase.username)
        : "";
      return urlBuild(
        resolvedBase.protocol, baseUserinfo, resolvedBase.hostname, resolvedBase.port, path, search, hash
      );
    }
    var protocol = scheme[1];
    var rest = protocolRelative ? value : value.substring(protocol.length);
    var hashIndex2 = rest.indexOf("#");
    var hash2 = "";
    if (hashIndex2 >= 0) { hash2 = rest.substring(hashIndex2); rest = rest.substring(0, hashIndex2); }
    var search2 = "";
    var queryIndex2 = rest.indexOf("?");
    if (queryIndex2 >= 0) { search2 = rest.substring(queryIndex2); rest = rest.substring(0, queryIndex2); }
    var authority2 = "";
    var path2 = rest;
    if (rest.substring(0, 2) === "//") {
      var afterSlashes = rest.substring(2);
      var end = afterSlashes.length;
      for (var i = 0; i < afterSlashes.length; i++) {
        if (afterSlashes.charAt(i) === "/") { end = i; break; }
      }
      authority2 = afterSlashes.substring(0, end);
      path2 = afterSlashes.substring(end);
    }
    var parts = urlSplitAuthority(authority2);
    return urlBuild(protocol, parts.userinfo, parts.hostname, parts.port, path2, search2, hash2);
  }
  function SearchParams(init) {
    this._list = [];
    if (typeof init === "string") {
      init.replace(/^\?/, "").split("&").forEach(function (pair) {
        if (!pair) { return; }
        var index = pair.indexOf("=");
        var key = index < 0 ? pair : pair.substring(0, index);
        var value = index < 0 ? "" : pair.substring(index + 1);
        this._list.push([decodeURIComponent(key.replace(/\+/g, " ")), decodeURIComponent(value.replace(/\+/g, " "))]);
      }, this);
    } else if (init && init._list) {
      this._list = init._list.map(function (pair) { return [pair[0], pair[1]]; });
    }
  }
  SearchParams.prototype.append = function (key, value) { this._list.push([String(key), String(value)]); };
  SearchParams.prototype["delete"] = function (key) { this._list = this._list.filter(function (pair) { return pair[0] !== String(key); }); };
  SearchParams.prototype.get = function (key) { for (var i = 0; i < this._list.length; i++) { if (this._list[i][0] === String(key)) { return this._list[i][1]; } } return null; };
  SearchParams.prototype.getAll = function (key) { return this._list.filter(function (pair) { return pair[0] === String(key); }).map(function (pair) { return pair[1]; }); };
  SearchParams.prototype.has = function (key) { return this.get(key) !== null; };
  SearchParams.prototype.set = function (key, value) { var found = false; var out = []; this._list.forEach(function (pair) { if (pair[0] === String(key)) { if (!found) { out.push([String(key), String(value)]); found = true; } } else { out.push(pair); } }); if (!found) { out.push([String(key), String(value)]); } this._list = out; };
  SearchParams.prototype.toString = function () { return this._list.map(function (pair) { return encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1]); }).join("&"); };
  SearchParams.prototype.sort = function () { this._list.sort(function (a, b) { return a[0] < b[0] ? -1 : (a[0] > b[0] ? 1 : 0); }); };
  SearchParams.prototype.forEach = function (callback, thisArg) { this._list.forEach(function (pair) { callback.call(thisArg, pair[1], pair[0], this); }, this); };
  define("URLSearchParams", SearchParams);
  function URL(input, base) {
    var parsed = urlParse(input, base);
    for (var key in parsed) { if (Object.prototype.hasOwnProperty.call(parsed, key)) { this[key] = parsed[key]; } }
    this.searchParams = new SearchParams(parsed.search);
  }
  URL.prototype.toString = function () { return this.href; };
  URL.prototype.toJSON = function () { return this.href; };
  define("URL", URL);
  var BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  define("btoa", function (input) {
    var value = String(input); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (value.charCodeAt(i) > 255) { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 3) {
      var a = value.charCodeAt(i), b = value.charCodeAt(i + 1), c = value.charCodeAt(i + 2);
      var triple = (a << 16) | ((isNaN(b) ? 0 : b) << 8) | (isNaN(c) ? 0 : c);
      out += BASE64.charAt((triple >> 18) & 63) + BASE64.charAt((triple >> 12) & 63) + (isNaN(b) ? "=" : BASE64.charAt((triple >> 6) & 63)) + (isNaN(c) ? "=" : BASE64.charAt(triple & 63));
    }
    return out;
  });
  define("atob", function (input) {
    var value = String(input).replace(/\s/g, ""); var out = "";
    for (var i = 0; i < value.length; i++) {
      if (BASE64.indexOf(value.charAt(i)) < 0 && value.charAt(i) !== "=") { throw new DOMException("InvalidCharacterError", "InvalidCharacterError"); }
    }
    for (var i = 0; i < value.length; i += 4) {
      var n = (BASE64.indexOf(value.charAt(i)) << 18) | (BASE64.indexOf(value.charAt(i + 1)) << 12) | ((value.charAt(i + 2) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 2))) << 6) | (value.charAt(i + 3) === "=" ? 0 : BASE64.indexOf(value.charAt(i + 3)));
      out += String.fromCharCode((n >> 16) & 255);
      if (value.charAt(i + 2) !== "=") { out += String.fromCharCode((n >> 8) & 255); }
      if (value.charAt(i + 3) !== "=") { out += String.fromCharCode(n & 255); }
    }
    return out;
  });
  define("alert", function (message) {
    if (typeof console !== "undefined" && console.log) { console.log(String(message)); }
  });
  define("confirm", function (message) {
    if (typeof console !== "undefined" && console.log) { console.log("confirm: " + String(message)); }
    return true;
  });
  define("prompt", function (message, defaultValue) {
    if (typeof console !== "undefined" && console.log) { console.log("prompt: " + String(message)); }
    return (defaultValue === undefined ? null : String(defaultValue));
  });
  function clone(value, seen) {
    if (value === null || typeof value !== "object") { return value; }
    if (seen.has(value)) { throw new DOMException("DataCloneError: cyclic object value", "DataCloneError"); }
    if (value instanceof Date) { return new Date(value.getTime()); }
    if (value instanceof Map) { var map = new Map(); seen.add(value); value.forEach(function (v, k) { map.set(clone(k, seen), clone(v, seen)); }); seen.delete(value); return map; }
    if (value instanceof Set) { var set = new Set(); seen.add(value); value.forEach(function (v) { set.add(clone(v, seen)); }); seen.delete(value); return set; }
    if (Array.isArray(value)) { seen.add(value); var arr = value.map(function (item) { return clone(item, seen); }); seen.delete(value); return arr; }
    seen.add(value); var out = {}; for (var key in value) { if (Object.prototype.hasOwnProperty.call(value, key)) { out[key] = clone(value[key], seen); } } seen.delete(value); return out;
  }
  define("structuredClone", function (value) { return clone(value, new Set()); });
  if (typeof Promise !== "undefined") { define("queueMicrotask", function (callback) { Promise.resolve().then(callback); }); }
  else { define("queueMicrotask", function (callback) { setTimeout(callback, 0); }); }
  define("requestIdleCallback", function (callback) { return setTimeout(function () { callback({ didTimeout: false, timeRemaining: function () { return 50; } }); }, 0); });
  define("cancelIdleCallback", function (id) { clearTimeout(id); });
  function makeMouseEvent(type, init) {
    init = init || {};
    this.__megi_event = true;
    this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable;
    this.relatedTarget = init.relatedTarget || null;
    this.clientX = init.clientX || 0; this.clientY = init.clientY || 0;
    this.button = init.button || 0; this.defaultPrevented = false;
  }
  makeMouseEvent.prototype.preventDefault = function () { this.defaultPrevented = true; };
  makeMouseEvent.prototype.stopPropagation = function () {};
  makeMouseEvent.prototype.stopImmediatePropagation = function () {};
  define("MouseEvent", makeMouseEvent);
  define("PointerEvent", function (type, init) { makeMouseEvent.call(this, type, init); this.pointerId = (init && init.pointerId) || 0; });
  if (typeof Event === "function" && Event.prototype) { define("PointerEvent", (function (Base) { function PointerEvent(type, init) { Base.call(this, type, init || {}); this.pointerId = (init && init.pointerId) || 0; } PointerEvent.prototype = Object.create(Base.prototype); PointerEvent.prototype.constructor = PointerEvent; return PointerEvent; })(Event)); }
  // KeyboardEvent / InputEvent / FocusEvent (#1504). Each derives from `Event` when available so
  // `type`, `bubbles`, `cancelable`, `preventDefault` and `stopPropagation` behave uniformly.
  function makeEvent(ctorName, Base, initFields) {
    function EventCtor(type, init) {
      init = init || {};
      if (Base) { Base.call(this, type, init); }
      else { this.__megi_event = true; this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable; this.defaultPrevented = false; }
      for (var key in initFields) { if (Object.prototype.hasOwnProperty.call(initFields, key)) { this[key] = init[key] === undefined ? initFields[key] : init[key]; } }
    }
    if (Base && Base.prototype) { EventCtor.prototype = Object.create(Base.prototype); }
    else { EventCtor.prototype = { preventDefault: function () { this.defaultPrevented = true; }, stopPropagation: function () {}, stopImmediatePropagation: function () {} }; }
    EventCtor.prototype.constructor = EventCtor;
    return EventCtor;
  }
  var EventBase = (typeof Event === "function" && Event.prototype) ? Event : null;
  define("KeyboardEvent", makeEvent("KeyboardEvent", EventBase, { key: '', code: '', ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, repeat: false }));
  define("InputEvent", makeEvent("InputEvent", EventBase, { data: null, inputType: '' }));
  define("FocusEvent", makeEvent("FocusEvent", EventBase, { relatedTarget: null }));
  define("Node", { DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4, DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32 });
  function Headers(init) {
    this._h = [];
    if (init) {
      if (init instanceof Headers) { this._h = init._h.map(function (p) { return [p[0], p[1]]; }); }
      else if (Array.isArray(init)) { for (var i = 0; i < init.length; i++) { this.append(init[i][0], init[i][1]); } }
      else { for (var key in init) { if (Object.prototype.hasOwnProperty.call(init, key)) { this.append(key, init[key]); } } }
    }
  }
  // Header names/values are serialized as `name:value\n` lines for the native side, so a CR,
  // LF or NUL inside either field would forge extra headers (or a colon inside the name would
  // re-split it). Validate at the boundary, like the fetch spec's header-name/header-value
  // grammar (#664).
  function headerName(value) {
    var name = String(value);
    if (/[\r\n\0:]/.test(name)) { throw new TypeError("invalid header name"); }
    return name;
  }
  function headerValue(value) {
    var text = String(value);
    if (/[\r\n\0]/.test(text)) { throw new TypeError("invalid header value"); }
    return text;
  }
  Headers.prototype._find = function (name) { var lower = String(name).toLowerCase(); for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { return i; } } return -1; };
  Headers.prototype.get = function (name) { var lower = String(name).toLowerCase(); var values = []; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { values.push(this._h[i][1]); } } return values.length === 0 ? null : values.join(", "); };
  Headers.prototype.has = function (name) { return this._find(name) >= 0; };
  Headers.prototype.append = function (name, value) { this._h.push([headerName(name), headerValue(value)]); };
  Headers.prototype.set = function (name, value) { var lower = headerName(name).toLowerCase(); var out = []; var found = false; for (var i = 0; i < this._h.length; i++) { if (this._h[i][0].toLowerCase() === lower) { if (!found) { out.push([this._h[i][0], headerValue(value)]); found = true; } } else { out.push(this._h[i]); } } if (!found) { out.push([headerName(name), headerValue(value)]); } this._h = out; };
  Headers.prototype["delete"] = function (name) { var lower = String(name).toLowerCase(); this._h = this._h.filter(function (pair) { return pair[0].toLowerCase() !== lower; }); };
  Headers.prototype.forEach = function (callback, thisArg) { for (var i = 0; i < this._h.length; i++) { callback.call(thisArg, this._h[i][1], this._h[i][0], this); } };
  Headers.prototype.entries = function () { return this._h.slice(); };
  define("Headers", Headers);
  function AbortSignal() {
    this.aborted = false;
    this.reason = undefined;
    this._listeners = [];
    this.onabort = null;
  }
  AbortSignal.prototype.addEventListener = function (type, callback) {
    if (type === "abort" && typeof callback === "function") { this._listeners.push(callback); }
  };
  AbortSignal.prototype.removeEventListener = function (type, callback) {
    if (type === "abort") { this._listeners = this._listeners.filter(function (item) { return item !== callback; }); }
  };
  AbortSignal.prototype.throwIfAborted = function () { if (this.aborted) { throw abortError(this.reason); } };
  AbortSignal.prototype._abort = function (reason) {
    if (this.aborted) { return; }
    this.aborted = true;
    this.reason = reason;
    var event = { type: "abort", target: this };
    if (typeof this.onabort === "function") { this.onabort(event); }
    for (var i = 0; i < this._listeners.length; i++) { this._listeners[i](event); }
  };
  define("AbortSignal", AbortSignal);
  function AbortController() { this.signal = new AbortSignal(); }
  AbortController.prototype.abort = function (reason) { this.signal._abort(reason); };
  define("AbortController", AbortController);
  function abortError(reason) {
    var error = new Error("The operation was aborted");
    error.name = "AbortError";
    if (reason !== undefined) { error.reason = reason; }
    return error;
  }
  function ReadableStream(chunks) {
    this._chunks = chunks || [];
    this._index = 0;
    this._cancelled = false;
    this.locked = false;
    this._onRead = null;
  }
  ReadableStream.prototype.getReader = function () {
    var stream = this;
    this.locked = true;
    return {
      read: function () {
        if (stream._onRead) { stream._onRead(); }
        if (stream._cancelled || stream._index >= stream._chunks.length) {
          return Promise.resolve({ value: undefined, done: true });
        }
        var chunk = stream._chunks[stream._index++];
        return Promise.resolve({ value: chunk, done: false });
      },
      cancel: function (reason) { stream.cancel(reason); return Promise.resolve(); },
      releaseLock: function () { stream.locked = false; },
      closed: Promise.resolve()
    };
  };
  ReadableStream.prototype.cancel = function (reason) {
    this._cancelled = true;
    this._chunks = [];
    this._index = 0;
    return Promise.resolve(reason);
  };
  define("ReadableStream", ReadableStream);
  function byteChunks(bytes) {
    var chunks = [];
    if (!bytes || bytes.length === 0) { return chunks; }
    var parts = Math.min(3, bytes.length);
    var size = Math.ceil(bytes.length / parts);
    for (var start = 0; start < bytes.length; start += size) {
      chunks.push(new Uint8Array(bytes.slice(start, start + size)));
    }
    return chunks;
  }
  function Response(data) {
    this.status = data.status;
    this.statusText = data.statusText || "";
    this.ok = data.status >= 200 && data.status < 300;
    this.url = data.url || "";
    this.redirected = Boolean(data.redirected);
    this.headers = new Headers(data.headers || []);
    this._body = data.bodyText || "";
    this._bytes = data.bodyBytes || [];
    this._bodyUsed = false;
    var self = this;
    this.body = new ReadableStream(byteChunks(this._bytes));
    this.body._onRead = function () { self._bodyUsed = true; };
  }
  Response.prototype._consume = function () { if (this._bodyUsed) { return null; } this._bodyUsed = true; return this._body; };
  Response.prototype.text = function () { var body = this._consume(); return body === null ? Promise.reject(new TypeError("body already used")) : Promise.resolve(body); };
  Response.prototype.json = function () { var body = this._consume(); if (body === null) { return Promise.reject(new TypeError("body already used")); } try { return Promise.resolve(JSON.parse(body)); } catch (e) { return Promise.reject(e); } };
  Response.prototype.arrayBuffer = function () {
    var body = this._consume();
    if (body === null) { return Promise.reject(new TypeError("body already used")); }
    return Promise.resolve(new Uint8Array(this._bytes.slice()));
  };
  Response.prototype.clone = function () {
    return new Response({ status: this.status, statusText: this.statusText, url: this.url, redirected: this.redirected, headers: this.headers.entries(), bodyText: this._body, bodyBytes: this._bytes.slice() });
  };
  define("Response", Response);
  function normalizeRedirect(value) {
    if (value === undefined) { return "follow"; }
    var s = String(value);
    if (s !== "follow" && s !== "manual" && s !== "error") {
      throw new TypeError("Invalid Request redirect mode: " + s);
    }
    return s;
  }
  define("__megi_test_pattern", function (pattern, value) {
    var re;
    try { re = new RegExp("^(?:" + pattern + ")$", "u"); }
    catch (_e1) {
      try { re = new RegExp("^(?:" + pattern + ")$"); }
      catch (_e2) { return true; }
    }
    return re.test(value);
  });
  function formControlDisabled(el, form) {
    if (el.disabled || (el.hasAttribute && el.hasAttribute("disabled"))) { return true; }
    var cur = el.parentNode;
    var child = el;
    while (cur && cur !== form) {
      if (cur.tagName && cur.tagName.toUpperCase() === "FIELDSET" && (cur.disabled || (cur.hasAttribute && cur.hasAttribute("disabled")))) {
        var firstLegend = null;
        var fsChildren = cur.children || [];
        for (var i = 0; i < fsChildren.length; i++) {
          if (fsChildren[i].tagName && fsChildren[i].tagName.toUpperCase() === "LEGEND") { firstLegend = fsChildren[i]; break; }
        }
        if (child !== firstLegend) { return true; }
      }
      child = cur;
      cur = cur.parentNode;
    }
    return false;
  }
  function collectFormControls(root, form, list) {
    var kids = root.children || [];
    for (var i = 0; i < kids.length; i++) {
      var el = kids[i];
      if (!el || !el.tagName) { continue; }
      var tag = el.tagName.toUpperCase();
      if (tag === "DATALIST") { continue; }
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || tag === "BUTTON") {
        list.push(el);
      }
      collectFormControls(el, form, list);
    }
  }
  function FormData(form, submitter) {
    this._list = [];
    this._boundary = "----megiFormBoundary7f3e9a2c";
    if (form !== undefined && form !== null) {
      if (!form.tagName || form.tagName.toUpperCase() !== "FORM") {
        throw new TypeError("Failed to construct 'FormData': parameter 1 is not of type 'HTMLFormElement'");
      }
      var controls = [];
      collectFormControls(form, form, controls);
      for (var i = 0; i < controls.length; i++) {
        var el = controls[i];
        if (formControlDisabled(el, form)) { continue; }
        var tag = el.tagName.toUpperCase();
        var name = el.getAttribute ? el.getAttribute("name") : null;
        if (!name) { continue; }
        if (tag === "BUTTON") {
          var btnType = (el.getAttribute("type") || "submit").toLowerCase();
          if (el === submitter && btnType !== "reset" && btnType !== "button") {
            this._list.push([name, String(el.value || "")]);
          }
          continue;
        }
        if (tag === "INPUT") {
          var type = (el.getAttribute("type") || "text").toLowerCase();
          if (type === "submit" || type === "reset" || type === "button" || type === "image") {
            if (el === submitter && type !== "reset" && type !== "button") {
              this._list.push([name, String(el.value || "")]);
            }
            continue;
          }
          if (type === "checkbox" || type === "radio") {
            if (!el.checked) { continue; }
            var rawVal = el.getAttribute("value");
            this._list.push([name, rawVal === null ? "on" : String(rawVal)]);
            continue;
          }
          if (type === "file") { continue; }
          this._list.push([name, String(el.value || "")]);
          continue;
        }
        if (tag === "TEXTAREA") {
          this._list.push([name, String(el.value || "")]);
          continue;
        }
        if (tag === "SELECT") {
          var multiple = el.hasAttribute && el.hasAttribute("multiple");
          var opts = el.options || [];
          if (multiple) {
            for (var j = 0; j < opts.length; j++) {
              if (opts[j].selected && !opts[j].disabled) {
                this._list.push([name, String(opts[j].value || "")]);
              }
            }
          } else {
            var picked = null;
            for (var j = 0; j < opts.length; j++) {
              if (opts[j].selected && !opts[j].disabled) { picked = opts[j]; break; }
            }
            if (!picked) {
              for (var j = 0; j < opts.length; j++) {
                if (!opts[j].disabled) { picked = opts[j]; break; }
              }
            }
            if (picked) {
              this._list.push([name, String(picked.value || "")]);
            }
          }
        }
      }
    }
  }
  function makeFormDataIter(list, mapFn) {
    var idx = 0;
    var iter = {
      next: function () {
        if (idx >= list.length) { return { value: undefined, done: true }; }
        return { value: mapFn(list[idx++]), done: false };
      }
    };
    if (typeof Symbol === "function" && Symbol.iterator) {
      iter[Symbol.iterator] = function () { return this; };
    }
    return iter;
  }
  FormData.prototype.append = function (name, value) {
    this._list.push([String(name), String(value)]);
  };
  FormData.prototype["delete"] = function (name) {
    var key = String(name);
    this._list = this._list.filter(function (pair) { return pair[0] !== key; });
  };
  FormData.prototype.get = function (name) {
    var key = String(name);
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) { return this._list[i][1]; }
    }
    return null;
  };
  FormData.prototype.getAll = function (name) {
    var key = String(name);
    var out = [];
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) { out.push(this._list[i][1]); }
    }
    return out;
  };
  FormData.prototype.has = function (name) { return this.get(name) !== null; };
  FormData.prototype.set = function (name, value) {
    var key = String(name);
    var val = String(value);
    var found = false;
    var out = [];
    for (var i = 0; i < this._list.length; i++) {
      if (this._list[i][0] === key) {
        if (!found) { out.push([key, val]); found = true; }
      } else {
        out.push(this._list[i]);
      }
    }
    if (!found) { out.push([key, val]); }
    this._list = out;
  };
  FormData.prototype.keys = function () {
    return makeFormDataIter(this._list, function (pair) { return pair[0]; });
  };
  FormData.prototype.values = function () {
    return makeFormDataIter(this._list, function (pair) { return pair[1]; });
  };
  FormData.prototype.entries = function () {
    return makeFormDataIter(this._list, function (pair) { return [pair[0], pair[1]]; });
  };
  FormData.prototype.forEach = function (callback, thisArg) {
    for (var i = 0; i < this._list.length; i++) {
      callback.call(thisArg, this._list[i][1], this._list[i][0], this);
    }
  };
  if (typeof Symbol === "function" && Symbol.iterator) {
    FormData.prototype[Symbol.iterator] = FormData.prototype.entries;
  }
  FormData.prototype._toMultipart = function () {
    var b = this._boundary;
    var out = "";
    for (var i = 0; i < this._list.length; i++) {
      var escapedName = this._list[i][0].replace(/[\r\n"]/g, function (c) {
        return c === '"' ? "%22" : (c === "\r" ? "%0D" : "%0A");
      });
      out += "--" + b + "\r\n";
      out += 'Content-Disposition: form-data; name="' + escapedName + '"\r\n\r\n';
      out += this._list[i][1] + "\r\n";
    }
    out += "--" + b + "--\r\n";
    return out;
  };
  define("FormData", FormData);
  function Request(input, init) {
    init = init || {};
    if (input && typeof input === "object" && typeof input.url === "string") {
      this.url = input.url;
      this.method = String(init.method || input.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers || input.headers);
      this.body = init.body !== undefined ? init.body : (input.body || null);
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || input.signal || null;
      this.redirect = normalizeRedirect(init.redirect !== undefined ? init.redirect : input.redirect);
    } else {
      this.url = new URL(String(input), location.href).href;
      this.method = String(init.method || "GET").toUpperCase();
      this.headers = new Headers(init.headers);
      this.body = init.body !== undefined && init.body !== null ? init.body : null;
      if (this.body !== null && (this.method === "GET" || this.method === "HEAD")) { throw new TypeError("Request with GET/HEAD method cannot have body"); }
      this.signal = init.signal || null;
      this.redirect = normalizeRedirect(init.redirect);
    }
    if (this.body !== null && typeof this.body === "object") {
      if (this.body instanceof FormData) {
        if (!this.headers.has("Content-Type")) {
          this.headers.set("Content-Type", "multipart/form-data; boundary=" + this.body._boundary);
        }
        this.body = this.body._toMultipart();
      } else if (this.body instanceof SearchParams) {
        if (!this.headers.has("Content-Type")) {
          this.headers.set("Content-Type", "application/x-www-form-urlencoded;charset=UTF-8");
        }
        this.body = this.body.toString();
      }
    }
  }
  define("Request", Request);
  define("fetch", function (input, init) {
    return new Promise(function (resolve, reject) {
      var request;
      try { request = new Request(input, init); } catch (error) { reject(error); return; }
      var signal = request.signal;
      if (signal && signal.aborted) { reject(abortError(signal.reason)); return; }
      var settled = false;
      var onAbort = function () { if (!settled) { settled = true; reject(abortError(signal.reason)); } };
      if (signal) { signal.addEventListener("abort", onAbort); }
      try {
        var headerText = "";
        request.headers.forEach(function (value, name) { headerText += name + ":" + value + "\n"; });
        var raw = __megi_fetch_native(request.url, request.method, headerText, request.body === null ? "" : request.body, request.redirect);
        Promise.resolve().then(function () {
          if (settled) { return; }
          settled = true;
          if (signal) { signal.removeEventListener("abort", onAbort); }
          var response = new Response(raw);
          if (request.redirect === "follow" && response.url && response.url !== request.url) {
            response.redirected = true;
          }
          if (request.method === "HEAD") { response._bytes = []; response._body = ""; response.body = new ReadableStream([]); }
          resolve(response);
        });
      } catch (error) {
        settled = true;
        if (signal) { signal.removeEventListener("abort", onAbort); }
        reject(new TypeError("fetch failed: " + String(error)));
      }
    });
  });


  // Intl.NumberFormat subset (#1523): locale-aware decimal formatting with grouping and
  // configurable fraction digits. Uses small per-locale separator tables (no ICU dependency);
  // unsupported locales fall back to en-US. Enough for the common currency/count display case.
  (function () {
    var separators = {
      'en': [',', '.'], 'en-us': [',', '.'], 'en-gb': [',', '.'],
      'es': ['.', ','], 'es-es': ['.', ','],
      'de': ['.', ','], 'de-de': ['.', ','],
      'fr': ['\u202f', ','], 'fr-fr': ['\u202f', ','],
      'pt': ['.', ','], 'pt-br': ['.', ',']
    };
    function NumberFormat(locale, options) {
      locale = String(locale || 'en-US');
      options = options || {};
      var pair = separators[locale.toLowerCase()] || separators[locale.split('-')[0].toLowerCase()] || separators['en'];
      this._group = pair[0];
      this._decimal = pair[1];
      this._minFrac = options.minimumFractionDigits === undefined ? 0 : Math.max(0, Math.min(20, options.minimumFractionDigits | 0));
      this._maxFrac = options.maximumFractionDigits === undefined
        ? Math.max(this._minFrac, 3)
        : Math.max(this._minFrac, Math.min(20, options.maximumFractionDigits | 0));
      this._grouping = options.useGrouping !== false;
      this._locale = locale;
    }
    NumberFormat.prototype.resolvedOptions = function () {
      return { locale: this._locale, minimumFractionDigits: this._minFrac, maximumFractionDigits: this._maxFrac, useGrouping: this._grouping };
    };
    NumberFormat.prototype.format = function (value) {
      var n = Number(value);
      if (!isFinite(n)) { return String(n); }
      var negative = n < 0;
      var fixed = Math.abs(n).toFixed(this._maxFrac);
      var parts = fixed.split('.');
      var integer = parts[0];
      var frac = parts[1] || '';
      while (frac.length > this._minFrac && frac.charAt(frac.length - 1) === '0') { frac = frac.slice(0, -1); }
      if (this._grouping) { integer = integer.replace(/\B(?=(\d{3})+(?!\d))/g, this._group); }
      return (negative ? '-' : '') + integer + (frac ? this._decimal + frac : '');
    };
    var intl = (typeof Intl === 'object' && Intl) ? Intl : {};
    intl.NumberFormat = NumberFormat;

    // Intl.DateTimeFormat subset (#1523): locale-ordered numeric dates/times. Kept
    // dependency-free (no ICU) and intentionally small: common `toLocale*` display cases.
    var dateOrder = {
      'en': ['m', 'd', 'y'], 'en-us': ['m', 'd', 'y'], 'en-gb': ['d', 'm', 'y'],
      'es': ['d', 'm', 'y'], 'es-es': ['d', 'm', 'y'],
      'de': ['d', 'm', 'y'], 'de-de': ['d', 'm', 'y'],
      'fr': ['d', 'm', 'y'], 'fr-fr': ['d', 'm', 'y'],
      'pt': ['d', 'm', 'y'], 'pt-br': ['d', 'm', 'y']
    };
    function DateTimeFormat(locale, options) {
      locale = String(locale || 'en-US');
      options = options || {};
      var key = locale.toLowerCase();
      this._order = dateOrder[key] || dateOrder[locale.split('-')[0].toLowerCase()] || dateOrder['en'];
      this._locale = locale;
      this._lang = locale.split('-')[0].toLowerCase();
      var hasDate = options.year || options.month || options.day || options.dateStyle;
      var hasTime = options.hour || options.minute || options.second || options.timeStyle;
      if (!hasDate && !hasTime) { hasDate = true; }
      this._date = !!hasDate;
      this._time = !!hasTime;
    }
    DateTimeFormat.prototype.resolvedOptions = function () {
      return { locale: this._locale, date: this._date, time: this._time };
    };
    DateTimeFormat.prototype.format = function (date) {
      var d = new Date(Number(date));
      if (isNaN(d.getTime())) { return 'Invalid Date'; }
      var parts = {
        y: String(d.getFullYear()),
        m: String(d.getMonth() + 1).padStart(2, '0'),
        d: String(d.getDate()).padStart(2, '0')
      };
      var datePart = this._order.map(function (key) { return parts[key]; }).join('/');
      var timePart = String(d.getHours()).padStart(2, '0') + ':' +
        String(d.getMinutes()).padStart(2, '0') + ':' +
        String(d.getSeconds()).padStart(2, '0');
      if (this._date && this._time) {
        return datePart + (this._lang === 'en' ? ', ' : ' ') + timePart;
      }
      if (this._time) { return timePart; }
      return datePart;
    };
    intl.DateTimeFormat = DateTimeFormat;
    define('Intl', intl);
    if (typeof Number === 'function' && Number.prototype) {
      Number.prototype.toLocaleString = function (locale, options) {
        return new NumberFormat(locale, options).format(this);
      };
      if (typeof Date === 'function' && Date.prototype) {
        Date.prototype.toLocaleDateString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options && (options.year || options.month || options.day || options.dateStyle)
              ? options : { year: 'numeric', month: '2-digit', day: '2-digit' }
          ).format(this);
        };
        Date.prototype.toLocaleTimeString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options || { hour: '2-digit', minute: '2-digit', second: '2-digit' }
          ).format(this);
        };
        Date.prototype.toLocaleString = function (locale, options) {
          return new DateTimeFormat(
            locale,
            options || {
              year: 'numeric', month: '2-digit', day: '2-digit',
              hour: '2-digit', minute: '2-digit', second: '2-digit'
            }
          ).format(this);
        };
      }
    }
  })();
})();
"####;
    let fetch_fn = FunctionObjectBuilder::new(context.realm(), native(fetch_native))
        .name(js_string!("__megi_fetch_native"))
        .length(4)
        .build();
    context
        .global_object()
        .set(
            js_string!("__megi_fetch_native"),
            JsValue::from(fetch_fn),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .eval(Source::from_bytes(PRELUDE))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))
}

#[cfg(test)]
mod dataset_name_tests {
    use super::*;

    #[test]
    pub(crate) fn dataset_camel_case_follows_domstringmap_rules() {
        assert_eq!(camel_case_attribute("data-foo-bar"), Some("fooBar".into()));
        assert_eq!(camel_case_attribute("data-a-é"), Some("a-é".into()));
        assert_eq!(camel_case_attribute("data-x-1"), Some("x-1".into()));
        // Per HTML spec, a dash followed by an ASCII lower alpha is removed even
        // when it is the first character of the suffix: "data--foo" -> "Foo".
        assert_eq!(camel_case_attribute("data--foo"), Some("Foo".into()));
        assert_eq!(camel_case_attribute("data-ß"), Some("ß".into()));
        assert_eq!(camel_case_attribute("role"), None);
    }
}
