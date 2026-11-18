use super::*;

// ---------------------------------------------------------------------------
// CSSOM: element.style and element.classList
// ---------------------------------------------------------------------------

/// Common CSS properties exposed by the inline `style` object (camelCase → CSS name).
macro_rules! style_prop_accessors {
    ($($get:ident, $set:ident => $css:literal),+ $(,)?) => {
        $(
            fn $get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                let node_id = this_node(this, context)?;
                let value = with_context_host(context, |host| {
                    inline_style_value(&host.document, node_id, $css)
                })
                .flatten();
                Ok(match value {
                    Some(value) => JsValue::from(js_string!(value)),
                    None => JsValue::from(js_string!("")),
                })
            }

            fn $set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                let node_id = this_node(this, context)?;
                let value = arg_string(args, 0, context)?;
                match with_context_host(context, |host| {
                    set_inline_style(&mut host.document, node_id, $css, &value)
                }) {
                    Some(Ok(())) => Ok(JsValue::undefined()),
                    Some(Err(message)) => Err(js_error(message)),
                    None => Err(js_error("no active JavaScript host")),
                }
            }
        )+
    };
}

style_prop_accessors!(
    style_color_get, style_color_set => "color",
    style_background_color_get, style_background_color_set => "background-color",
    style_font_size_get, style_font_size_set => "font-size",
    style_width_get, style_width_set => "width",
    style_height_get, style_height_set => "height",
    style_display_get, style_display_set => "display",
);

pub(crate) fn style_object(context: &mut Context, handle: u32) -> JsValue {
    let accessors: [(&str, NativeFn, NativeFn); 6] = [
        ("color", style_color_get, style_color_set),
        (
            "backgroundColor",
            style_background_color_get,
            style_background_color_set,
        ),
        ("fontSize", style_font_size_get, style_font_size_set),
        ("width", style_width_get, style_width_set),
        ("height", style_height_get, style_height_set),
        ("display", style_display_get, style_display_set),
    ];
    let mut prepared = Vec::with_capacity(accessors.len());
    for (name, get, set) in accessors {
        prepared.push((name, accessor_fn(context, get), accessor_fn(context, set)));
    }
    let css_text_get = accessor_fn(context, style_css_text_get);
    let css_text_set = accessor_fn(context, style_css_text_set);
    let length_get = accessor_fn(context, style_length_get);
    let methods: [(&str, usize, NativeFn); 4] = [
        ("getPropertyValue", 1, style_get_property_value),
        ("setProperty", 1, style_set_property),
        ("removeProperty", 1, style_remove_property),
        ("item", 1, style_item),
    ];
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::all(),
    );
    for (name, get, set) in prepared {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    init.accessor(
        js_string!("cssText"),
        Some(css_text_get),
        Some(css_text_set),
        Attribute::all(),
    );
    init.accessor(
        js_string!("length"),
        Some(length_get),
        None,
        Attribute::all(),
    );
    for (name, length, function) in methods {
        init.function(native(function), js_string!(name), length);
    }
    JsValue::from(init.build())
}

/// Resolves the inline-style node from a `style` object receiver.
pub(crate) fn style_node(this: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("style receiver is not an object"))?;
    let handle = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("style receiver is not an element"))?;
    with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

pub(crate) fn inline_declarations(doc: &Document, node_id: NodeId) -> Vec<megi_css::Declaration> {
    doc.try_node(node_id)
        .and_then(|node| node.as_element())
        .and_then(|element| element.get_attribute("style"))
        .map(megi_css::parser::parse_list_of_declarations)
        .unwrap_or_default()
}

pub(crate) fn write_inline_declarations(
    doc: &mut Document,
    node_id: NodeId,
    declarations: &[megi_css::Declaration],
) -> Result<(), String> {
    let text = declarations
        .iter()
        .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
        .collect::<Vec<_>>()
        .join(" ");
    let element = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .ok_or_else(|| "node is not an element".to_string())?;
    element.set_attribute(DomAttribute::new("style", text), true);
    Ok(())
}

pub(crate) fn style_css_text_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let text = with_context_host(context, |host| {
        inline_declarations(&host.document, node)
            .iter()
            .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
            .collect::<Vec<_>>()
            .join(" ")
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(text)))
}

pub(crate) fn style_css_text_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let text = arg_string(args, 0, context)?;
    let declarations = megi_css::parser::parse_list_of_declarations(&text);
    match with_context_host(context, |host| {
        write_inline_declarations(&mut host.document, node, &declarations)
    }) {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn style_length_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let length = with_context_host(context, |host| {
        inline_declarations(&host.document, node).len()
    })
    .unwrap_or(0);
    Ok(JsValue::from(length as f64))
}

pub(crate) fn style_get_property_value(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = with_context_host(context, |host| {
        inline_style_value(&host.document, node, &name)
    })
    .flatten();
    Ok(JsValue::from(js_string!(value.unwrap_or_default())))
}

pub(crate) fn style_set_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let value = arg_string(args, 1, context)?;
    match with_context_host(context, |host| {
        set_inline_style(&mut host.document, node, &name, &value)
    }) {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn style_remove_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let name = arg_string(args, 0, context)?;
    let previous = with_context_host(context, |host| {
        let mut declarations = inline_declarations(&host.document, node);
        let previous = declarations
            .iter()
            .find(|declaration| declaration.name.eq_ignore_ascii_case(&name))
            .map(|declaration| declaration.raw_value.clone())
            .unwrap_or_default();
        declarations.retain(|declaration| !declaration.name.eq_ignore_ascii_case(&name));
        let _ = write_inline_declarations(&mut host.document, node, &declarations);
        previous
    })
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(previous)))
}

pub(crate) fn style_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = style_node(this, context)?;
    let index = crate::events::webidl_uint32_index(args, context);
    let name = with_context_host(context, |host| {
        inline_declarations(&host.document, node)
            .into_iter()
            .nth(index)
            .map(|declaration| declaration.name)
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(name)))
}

pub(crate) fn inline_style_value(
    doc: &Document,
    node_id: NodeId,
    property: &str,
) -> Option<String> {
    let style_text = doc.try_node(node_id).and_then(|node| {
        node.as_element()
            .and_then(|element| element.get_attribute("style"))
    })?;
    let declarations = megi_css::parser::parse_list_of_declarations(style_text);
    declarations
        .iter()
        .find(|declaration| declaration.name.eq_ignore_ascii_case(property))
        .map(|declaration| declaration.raw_value.clone())
}

pub(crate) fn set_inline_style(
    doc: &mut Document,
    node_id: NodeId,
    property: &str,
    value: &str,
) -> Result<(), String> {
    let style_text = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .map(|element| element.get_attribute("style").map(str::to_owned))
        .ok_or_else(|| "node is not an element".to_string())?
        .unwrap_or_default();
    let mut declarations = megi_css::parser::parse_list_of_declarations(&style_text);
    declarations.retain(|declaration| !declaration.name.eq_ignore_ascii_case(property));
    declarations.push(megi_css::Declaration {
        name: property.to_ascii_lowercase(),
        value: Vec::new(),
        important: false,
        raw_value: value.to_string(),
    });
    let style = declarations
        .iter()
        .map(|declaration| format!("{}: {};", declaration.name, declaration.raw_value))
        .collect::<Vec<_>>()
        .join(" ");
    let element = doc
        .try_node_mut(node_id)
        .and_then(|node| node.as_element_mut())
        .ok_or_else(|| "node is not an element".to_string())?;
    element.set_attribute(DomAttribute::new("style", style), true);
    Ok(())
}

pub(crate) fn class_list_object(context: &mut Context, handle: u32) -> JsValue {
    let length = accessor_fn(context, class_length);
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::all(),
    );
    for (name, length, function) in [
        (
            "add",
            1usize,
            class_add as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("remove", 1, class_remove),
        ("contains", 1, class_contains),
        ("toggle", 1, class_toggle),
        ("toString", 0, class_to_string),
        ("item", 1, class_item),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    init.accessor(js_string!("length"), Some(length), None, Attribute::all());
    JsValue::from(init.build())
}

pub(crate) fn classes_of(this: &JsValue, context: &mut Context) -> JsResult<(NodeId, Vec<String>)> {
    let node_id = this_node(this, context)?;
    let classes = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute("class"))
            })
            .map(|value| value.split_ascii_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    })
    .unwrap_or_default();
    Ok((node_id, classes))
}

pub(crate) fn write_classes(node_id: NodeId, classes: &[String], context: &mut Context) {
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("class", classes.join(" ")), true);
        }
    });
}

pub(crate) fn validate_class_token(token: &str) -> JsResult<()> {
    if token.is_empty() {
        return Err(syntax_error("The token provided must not be empty."));
    }
    if token.chars().any(|ch| ch.is_ascii_whitespace()) {
        return Err(js_error(
            "InvalidCharacterError: The token provided contains HTML space characters.",
        ));
    }
    Ok(())
}

pub(crate) fn class_add(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    let mut parsed = Vec::with_capacity(args.len());
    for index in 0..args.len() {
        let class = arg_string(args, index, context)?;
        validate_class_token(&class)?;
        parsed.push(class);
    }
    for class in parsed {
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    write_classes(node_id, &classes, context);
    Ok(JsValue::undefined())
}

pub(crate) fn class_remove(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    let mut parsed = Vec::with_capacity(args.len());
    for index in 0..args.len() {
        let class = arg_string(args, index, context)?;
        validate_class_token(&class)?;
        parsed.push(class);
    }
    for class in parsed {
        classes.retain(|existing| *existing != class);
    }
    write_classes(node_id, &classes, context);
    Ok(JsValue::undefined())
}

pub(crate) fn class_contains(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    let class = arg_string(args, 0, context)?;
    Ok(JsValue::from(classes.contains(&class)))
}

pub(crate) fn class_toggle(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (node_id, mut classes) = classes_of(this, context)?;
    let class = arg_string(args, 0, context)?;
    validate_class_token(&class)?;
    let contains = classes.contains(&class);
    let add = match args.get(1) {
        Some(force) if !force.is_undefined() => force.to_boolean(),
        _ => !contains,
    };
    if add {
        if !contains {
            classes.push(class);
        }
    } else {
        classes.retain(|existing| *existing != class);
    }
    write_classes(node_id, &classes, context);
    Ok(JsValue::from(add))
}

pub(crate) fn class_to_string(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    Ok(JsValue::from(js_string!(classes.join(" "))))
}

pub(crate) fn class_length(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (_, classes) = classes_of(this, context)?;
    Ok(JsValue::from(classes.len() as f64))
}
