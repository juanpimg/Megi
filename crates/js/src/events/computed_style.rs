use super::*;

pub(crate) fn get_computed_style(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = args
        .first()
        .and_then(|value| value.as_object())
        .and_then(|object| {
            prop(&object, "__megi_node", context)
                .ok()
                .and_then(|value| value.as_number())
                .map(|value| value as u32)
        })
        .ok_or_else(|| js_error("getComputedStyle expects an element"))?;
    let node_id = node_id_of(context, node)?;
    // Clone the Arc from the immutable provider slot. Providers are `'static +
    // Send + Sync` closures with no `&mut Context`, so they cannot re-enter
    // this realm; calling under a short shared borrow is safe and avoids an
    // O(arena) deep clone of the document on every getComputedStyle call.
    let provider = provider_or_mirror(
        context,
        |slot| slot.computed_style.clone(),
        |host| host.computed_style_provider.clone(),
    );
    let pairs = match provider {
        Some(provider) => provider(&host_ref(context)?.document, node_id),
        None => {
            // Fallback reads the inline `style` attribute with its own short borrow.
            host_ref(context)?
                .document
                .try_node(node_id)
                .and_then(|node| node.as_element())
                .and_then(|element| element.get_attribute("style"))
                .map(|style| {
                    megi_css::parser::parse_list_of_declarations(style)
                        .into_iter()
                        .map(|declaration| (declaration.name, declaration.raw_value))
                        .collect()
                })
                .unwrap_or_default()
        }
    };
    let getter = accessor_fn(context, computed_get_property);
    let setter = accessor_fn(context, computed_set_property);
    let item = accessor_fn(context, computed_item);
    let names = JsArray::from_iter([], context);
    for (name, _value) in &pairs {
        let _ = names.push(js_string!(name.clone()), context);
    }
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_computed"), true, Attribute::all())
        .property(
            js_string!("__megi_computed_names"),
            JsValue::from(names),
            Attribute::all(),
        );
    for (name, value) in &pairs {
        init.property(
            js_string!(name.clone()),
            js_string!(value.clone()),
            Attribute::all(),
        );
    }
    init.property(
        js_string!("length"),
        f64::from(pairs.len() as u32),
        Attribute::all(),
    )
    .function(
        NativeFunction::from_fn_ptr(computed_get_property),
        js_string!("getPropertyValue"),
        1,
    )
    .function(
        NativeFunction::from_fn_ptr(computed_set_property),
        js_string!("setProperty"),
        2,
    )
    .function(
        NativeFunction::from_fn_ptr(computed_item),
        js_string!("item"),
        1,
    );
    let css_text = pairs
        .iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ");
    init.property(
        js_string!("cssText"),
        js_string!(css_text),
        Attribute::all(),
    );
    let _ = (getter, setter, item);
    Ok(JsValue::from(init.build()))
}

pub(crate) fn computed_get_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = arg_string(args, 0, context)?;
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid computed style receiver"))?;
    if matches!(
        name.as_str(),
        "__megi_computed"
            | "__megi_computed_names"
            | "length"
            | "getPropertyValue"
            | "setProperty"
            | "item"
            | "cssText"
    ) {
        return Ok(JsValue::from(js_string!("")));
    }
    let has_own = object
        .has_own_property(js_string!(name.clone()), context)
        .unwrap_or(false);
    if !has_own {
        return Ok(JsValue::from(js_string!("")));
    }
    let value = prop(&object, &name, context).unwrap_or_else(|_| JsValue::from(js_string!("")));
    if let Some(s) = value.as_string() {
        Ok(JsValue::from(s.clone()))
    } else {
        Ok(JsValue::from(js_string!("")))
    }
}

pub(crate) fn computed_set_property(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Err(js_error("computed styles are read-only"))
}

pub(crate) fn webidl_uint32_index(args: &[JsValue], context: &mut Context) -> usize {
    let Some(arg) = args.first() else {
        return 0;
    };
    if arg.is_undefined() {
        return 0;
    }
    let Some(num) = arg.to_number(context).ok() else {
        return 0;
    };
    if !num.is_finite() || num == 0.0 {
        return 0;
    }
    num.trunc().rem_euclid(4_294_967_296.0) as u32 as usize
}

pub(crate) fn computed_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let index = webidl_uint32_index(args, context);
    let object = this
        .as_object()
        .ok_or_else(|| js_error("invalid computed style receiver"))?;
    let names = prop(&object, "__megi_computed_names", context)?;
    let names = names
        .as_object()
        .ok_or_else(|| js_error("computed style names missing"))?;
    let name = names
        .get(js_string!(index.to_string()), context)
        .map_err(|error| js_error(error.to_string()))?;
    if name.is_undefined() {
        Ok(JsValue::from(js_string!("")))
    } else {
        Ok(name)
    }
}

// ---------------------------------------------------------------------------
// Timers
// ---------------------------------------------------------------------------
