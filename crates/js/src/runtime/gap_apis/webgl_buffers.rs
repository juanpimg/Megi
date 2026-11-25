//! Buffer-object WebGL 1 bindings used by the vendored conformance subset.
//!
//! Spec: WebGL 1.0 §5.14.4–§5.14.6. The buffer bytes are copied into the
//! canvas-owned state; no JS handle crosses a crate boundary and no `unsafe`
//! or reference-counted state is introduced.

use super::canvas_support::{canvas_context_node, canvas_number};
use super::*;
use crate::canvas::ops as canvas_ops;
use crate::canvas::webgl;
use crate::runtime::buffer_source::extract_buffer_source_bytes;

/// WebIDL `long` conversion: non-finite values become zero (WebGL 1.0 §5.14.5).
fn buffer_offset(value: Option<&JsValue>, context: &mut Context) -> JsResult<usize> {
    let number = value
        .map(|value| value.to_number(context))
        .transpose()?
        .unwrap_or(0.0);
    if !number.is_finite() || number <= 0.0 {
        return Ok(0);
    }
    Ok((number as u64).min(webgl::MAX_BUFFER_BYTES as u64) as usize)
}

/// WebGL 1.0 §5.14.6.
pub(crate) fn webgl_get_error(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let error = canvas_ops::surface_value(context, node, |surface| surface.webgl_get_error())
        .unwrap_or(webgl::INVALID_OPERATION);
    Ok(JsValue::from(error))
}

/// WebGL 1.0 §5.14.4.
pub(crate) fn webgl_create_buffer(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let handle = canvas_ops::surface_value(context, node, |surface| surface.webgl_create_buffer())
        .unwrap_or(0);
    let mut object = ObjectInitializer::new(context);
    object.property(
        js_string!("__megi_webgl_buffer"),
        JsValue::from(handle),
        Attribute::all(),
    );
    Ok(object.build().into())
}

fn buffer_handle(value: Option<&JsValue>, context: &mut Context) -> JsResult<u32> {
    let Some(object) = value.and_then(JsValue::as_object) else {
        return Ok(0);
    };
    let handle = object
        .get(js_string!("__megi_webgl_buffer"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0 && *number <= u32::MAX as f64 && number.fract() == 0.0)
        .map(|number| number as u32)
        .unwrap_or(0);
    Ok(handle)
}

/// WebGL 1.0 §5.14.4.
pub(crate) fn webgl_bind_buffer(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let target = canvas_number(args, 0, context)? as u32;
    let buffer = buffer_handle(args.get(1), context)?;
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_bind_buffer(target, buffer);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §5.14.4.
pub(crate) fn webgl_buffer_data(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let target = canvas_number(args, 0, context)? as u32;
    let usage = if args.len() > 2 {
        canvas_number(args, 2, context)? as u32
    } else {
        webgl::STATIC_DRAW
    };
    let bytes = args
        .get(1)
        .and_then(JsValue::as_object)
        .map(|object| extract_buffer_source_bytes(&object, context))
        .transpose()?
        .flatten()
        .unwrap_or_default();
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_buffer_data(target, &bytes, usage);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §5.14.5.
pub(crate) fn webgl_buffer_sub_data(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let target = canvas_number(args, 0, context)? as u32;
    let offset = buffer_offset(args.get(1), context)?;
    let bytes = args
        .get(2)
        .and_then(JsValue::as_object)
        .map(|object| extract_buffer_source_bytes(&object, context))
        .transpose()?
        .flatten()
        .unwrap_or_default();
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_buffer_sub_data(target, offset, &bytes);
    });
    Ok(JsValue::undefined())
}
