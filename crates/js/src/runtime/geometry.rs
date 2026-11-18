use super::*;

// ---------------------------------------------------------------------------
// Layout geometry provider (round 28)
// ---------------------------------------------------------------------------

pub(crate) fn node_layout_rect(
    this: &JsValue,
    context: &mut Context,
) -> JsResult<Option<crate::host::LayoutRect>> {
    let node_id = this_node(this, context)?;
    let provider = ctx_or_mirror(
        context,
        |slot| slot.layout.clone(),
        |host| host.layout_provider.clone(),
    );
    Ok(provider.and_then(|provider| provider(node_id)))
}

pub(crate) fn rect_from_border(rect: &crate::host::LayoutRect) -> (f64, f64, f64, f64) {
    let (x, y, width, height) = rect.border_box;
    (
        f64::from(x),
        f64::from(y),
        f64::from(width),
        f64::from(height),
    )
}

pub(crate) fn node_get_bounding_client_rect(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let rect = node_layout_rect(this, context)?;
    let (x, y, width, height) = rect
        .as_ref()
        .map(rect_from_border)
        .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("x"), JsValue::from(x), Attribute::all())
        .property(js_string!("y"), JsValue::from(y), Attribute::all())
        .property(js_string!("width"), JsValue::from(width), Attribute::all())
        .property(
            js_string!("height"),
            JsValue::from(height),
            Attribute::all(),
        )
        .property(js_string!("top"), JsValue::from(y), Attribute::all())
        .property(js_string!("left"), JsValue::from(x), Attribute::all())
        .property(
            js_string!("right"),
            JsValue::from(x + width),
            Attribute::all(),
        )
        .property(
            js_string!("bottom"),
            JsValue::from(y + height),
            Attribute::all(),
        );
    Ok(JsValue::from(init.build()))
}

pub(crate) fn layout_metric(
    this: &JsValue,
    context: &mut Context,
    border: bool,
    width: bool,
) -> JsResult<JsValue> {
    let rect = node_layout_rect(this, context)?;
    let Some(rect) = rect else {
        return Ok(JsValue::from(0.0));
    };
    let (x, y, w, h) = if border {
        rect.border_box
    } else {
        rect.content_box
    };
    let value = if width { w } else { h };
    let _ = (x, y);
    Ok(JsValue::from((value.round() as i64) as f64))
}

pub(crate) fn node_offset_width(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    layout_metric(this, c, true, true)
}
pub(crate) fn node_offset_height(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    layout_metric(this, c, true, false)
}
pub(crate) fn node_client_width(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    layout_metric(this, c, false, true)
}
pub(crate) fn node_client_height(
    this: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    layout_metric(this, c, false, false)
}

pub(crate) fn viewport_metric(context: &mut Context, width: bool) -> JsResult<JsValue> {
    let provider = ctx_or_mirror(
        context,
        |slot| slot.viewport.clone(),
        |host| host.viewport_provider.clone(),
    );
    let value = provider
        .map(|provider| {
            let info = provider();
            if width { info.width } else { info.height }
        })
        .unwrap_or(if width { 1280 } else { 720 });
    Ok(JsValue::from(f64::from(value)))
}

pub(crate) fn window_inner_width(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    viewport_metric(c, true)
}
pub(crate) fn window_inner_height(
    _t: &JsValue,
    _a: &[JsValue],
    c: &mut Context,
) -> JsResult<JsValue> {
    viewport_metric(c, false)
}
pub(crate) fn window_device_pixel_ratio(
    _t: &JsValue,
    _a: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let provider = ctx_or_mirror(
        context,
        |slot| slot.viewport.clone(),
        |host| host.viewport_provider.clone(),
    );
    let ratio = provider
        .map(|provider| provider().device_pixel_ratio)
        .unwrap_or(1.0);
    Ok(JsValue::from(ratio))
}
