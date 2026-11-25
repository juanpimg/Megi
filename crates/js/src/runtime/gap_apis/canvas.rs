use super::canvas_support::{canvas_context_node, canvas_number};
use super::*;

/// Rejects a JS-supplied `ImageData.data.length` that is not a bounded integer count (#1774).
/// `1e30` cast to `u32` saturates and would drive billions of native loop iterations.
fn bounded_data_len(raw: f64, expected: usize) -> Option<usize> {
    if !raw.is_finite() || raw < 0.0 || raw.fract() != 0.0 || raw > expected as f64 {
        None
    } else {
        Some(raw as usize)
    }
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
        ("beginPath", canvas_begin_path as NativeFn),
        ("arc", canvas_arc as NativeFn),
        ("fill", canvas_fill as NativeFn),
        ("fillText", canvas_fill_text as NativeFn),
        ("strokeText", canvas_fill_text as NativeFn),
        ("measureText", canvas_measure_text as NativeFn),
        ("getImageData", canvas_get_image_data as NativeFn),
        ("putImageData", canvas_put_image_data as NativeFn),
    ] {
        init.function(native(function), js_string!(name), 4);
    }
    super::canvas_paths::register_canvas_2d_methods(&mut init);
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
    // globalAlpha accessor (#1961, WHATWG HTML §4.12.5.1.12).
    let get_alpha = accessor_fn(context, canvas_global_alpha_get);
    let set_alpha = accessor_fn(context, canvas_global_alpha_set);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(get_alpha))
        .set(JsValue::from(set_alpha))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = context_object.define_property_or_throw(js_string!("globalAlpha"), descriptor, context);
    super::canvas_paths::register_canvas_2d_accessors(&context_object, context);
    Ok(context_object.into())
}

pub(crate) fn canvas_begin_path(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    with_context_host(context, |host| {
        host.canvas_store.with_surface_mut(node, |s| s.begin_path())
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_arc(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 0, context)?;
    let y = canvas_number(args, 1, context)?;
    let radius = canvas_number(args, 2, context)?;
    let start = canvas_number(args, 3, context)?;
    let end = canvas_number(args, 4, context)?;
    let ccw = args.get(5).is_some_and(JsValue::to_boolean);
    with_context_host(context, |host| {
        host.canvas_store
            .with_surface_mut(node, |s| s.arc(x, y, radius, start, end, ccw))
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_fill(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    with_context_host(context, |host| {
        host.canvas_store.with_surface_mut(node, |s| s.fill_path())
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_fill_text(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 1, context)?;
    let y = canvas_number(args, 2, context)?;
    let text = args
        .first()
        .map(|value| value.to_string(context))
        .transpose()?
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default();
    with_context_host(context, |host| {
        host.canvas_store
            .with_surface_mut(node, |s| s.fill_text(x, y, &text))
    });
    Ok(JsValue::undefined())
}

pub(crate) fn canvas_measure_text(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let text = args
        .first()
        .map(|value| value.to_string(context))
        .transpose()?
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default();
    let width = crate::canvas::CanvasSurface::measure_text(&text);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("width"), JsValue::from(width), Attribute::all());
    Ok(JsValue::from(init.build()))
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
    super::canvas_paths::set_paint_style(node, args.first(), true, context)?;
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
    super::canvas_paths::set_paint_style(node, args.first(), false, context)?;
    Ok(JsValue::undefined())
}

/// `globalAlpha` getter (#1961). Returns 1.0 when no surface exists yet.
pub(crate) fn canvas_global_alpha_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let alpha = with_context_host(context, |host| {
        host.canvas_store.get(node).map(|s| s.global_alpha)
    })
    .flatten()
    .unwrap_or(1.0);
    Ok(JsValue::from(f64::from(alpha)))
}

/// `globalAlpha` setter (#1961, WHATWG HTML §4.12.5.1.12): values outside [0, 1]
/// are ignored, leaving the previous alpha unchanged.
pub(crate) fn canvas_global_alpha_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let Some(raw) = args.first().and_then(|value| value.as_number()) else {
        return Ok(JsValue::undefined());
    };
    if raw.is_finite() && (0.0..=1.0).contains(&raw) {
        with_context_host(context, |host| {
            host.canvas_store
                .with_surface_mut(node, |s| s.global_alpha = raw as f32)
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
    let expected = (w as usize).saturating_mul(h as usize).saturating_mul(4);
    let len = data_obj
        .as_ref()
        .and_then(|obj| {
            obj.get(js_string!("length"), context)
                .ok()
                .and_then(|v| v.as_number())
        })
        .and_then(|raw| bounded_data_len(raw, expected));
    let Some(len) = len else {
        return Ok(JsValue::undefined());
    };
    let obj = data_obj.as_ref().expect("obj");
    let fits_surface = with_context_host(context, |host| {
        host.canvas_store.get(node).is_some_and(|s| {
            dx >= 0
                && dy >= 0
                && (dx as u64) + u64::from(w) <= u64::from(s.width)
                && (dy as u64) + u64::from(h) <= u64::from(s.height)
                && w > 0
                && h > 0
        })
    })
    .unwrap_or(false);
    if !fits_surface {
        return Ok(JsValue::undefined());
    }
    let bytes: Vec<u8> =
        match crate::runtime::buffer_source::extract_u8_typed_array_bytes(obj, len, context) {
            Some(fast) => fast,
            None => (0..len)
                .filter_map(|i| {
                    obj.get(js_string!(i.to_string()), context)
                        .ok()
                        .and_then(|v| v.as_number())
                })
                .map(|n| n.clamp(0.0, 255.0) as u8)
                .collect(),
        };
    with_context_host(context, |host| {
        host.canvas_store.with_surface_mut(node, |s| {
            s.operations = s.operations.saturating_add(len as u64);
            let _ = s.put_image_data(dx, dy, w, h, &bytes);
        })
    });
    Ok(JsValue::undefined())
}

#[cfg(test)]
mod tests {
    use super::bounded_data_len;

    #[test]
    fn put_image_data_length_is_bounded_by_the_surface() {
        // #1774: a JS-supplied `length` of 1e30 must not drive billions of native iterations.
        assert_eq!(bounded_data_len(1e30, 16), None);
        assert_eq!(bounded_data_len(f64::NAN, 16), None);
        assert_eq!(bounded_data_len(-1.0, 16), None);
        assert_eq!(bounded_data_len(1.5, 16), None);
        assert_eq!(bounded_data_len(17.0, 16), None);
        assert_eq!(bounded_data_len(16.0, 16), Some(16));
    }
}
