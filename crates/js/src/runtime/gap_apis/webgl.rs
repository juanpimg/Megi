//! Software WebGL 1 context bindings (WebGL 1.0 §5.14, §6.2).
//!
//! The context object carries the same `__megi_canvas_node` handle as a 2D
//! context and mutates the surface through the narrow [`CanvasOps`] slot, so
//! this module never names `Host` and adds no capability to that boundary.

use super::canvas_support::{canvas_context_node, canvas_number};
use super::webgl_buffers::{
    webgl_bind_buffer, webgl_buffer_data, webgl_buffer_sub_data, webgl_create_buffer,
    webgl_delete_buffer, webgl_get_attrib_location, webgl_get_error, webgl_vertex_attrib_pointer,
};
use super::*;
use crate::canvas::ops as canvas_ops;
use crate::canvas::webgl;

/// Reads a bounded non-negative index/count argument (DoS guard, WebGL 1.0 §6.2).
fn bounded_index(value: Option<&JsValue>, _context: &mut Context) -> usize {
    value
        .and_then(|value| value.as_number())
        .filter(|number| number.is_finite() && *number >= 0.0)
        .map(|number| {
            (number as u64)
                .min(webgl::MAX_DRAW_VERTICES as u64)
                .min(usize::MAX as u64) as usize
        })
        .unwrap_or(0)
}

/// Copies bytes into a JS `Uint8Array`, honoring byteOffset (WebGL 1.0 §6.2).
fn write_u8_array(destination: &JsObject, bytes: &[u8], context: &mut Context) -> bool {
    let Ok(array) = JsUint8Array::from_object(destination.clone()) else {
        return false;
    };
    let Ok(buffer) = array.buffer(context) else {
        return false;
    };
    let Some(object) = buffer.as_object() else {
        return false;
    };
    let Ok(buffer) = JsArrayBuffer::from_object(object.clone()) else {
        return false;
    };
    let offset = array.byte_offset(context).unwrap_or(0);
    let available = array.byte_length(context).unwrap_or(0);
    if bytes.len() > available || bytes.len() > buffer.byte_length().saturating_sub(offset) {
        return false;
    }
    let Some(mut data) = buffer.data_mut() else {
        return false;
    };
    let end = offset.saturating_add(bytes.len());
    if end > data.len() {
        return false;
    }
    data[offset..end].copy_from_slice(bytes);
    true
}

/// Builds the `WebGLRenderingContext` wrapper for a canvas node.
pub(crate) fn build_webgl_context(handle: u32, context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_canvas_node"),
        JsValue::from(handle),
        Attribute::all(),
    );
    for (name, value) in [
        ("COLOR_BUFFER_BIT", webgl::COLOR_BUFFER_BIT),
        ("DEPTH_BUFFER_BIT", webgl::DEPTH_BUFFER_BIT),
        ("POINTS", webgl::POINTS),
        ("TRIANGLES", webgl::TRIANGLES),
        ("DEPTH_TEST", webgl::DEPTH_TEST),
        ("RGBA", webgl::RGBA),
        ("UNSIGNED_BYTE", webgl::UNSIGNED_BYTE),
        ("ARRAY_BUFFER", webgl::ARRAY_BUFFER),
        ("STATIC_DRAW", webgl::STATIC_DRAW),
        ("FLOAT", webgl::FLOAT),
    ] {
        init.property(js_string!(name), JsValue::from(value), Attribute::all());
    }
    for (name, function) in [
        ("clearColor", webgl_clear_color as NativeFn),
        ("clear", webgl_clear as NativeFn),
        ("viewport", webgl_viewport as NativeFn),
        ("enable", webgl_enable as NativeFn),
        ("drawArrays", webgl_draw_arrays as NativeFn),
        ("readPixels", webgl_read_pixels as NativeFn),
        ("getError", webgl_get_error as NativeFn),
        ("createBuffer", webgl_create_buffer as NativeFn),
        ("bindBuffer", webgl_bind_buffer as NativeFn),
        ("bufferData", webgl_buffer_data as NativeFn),
        ("bufferSubData", webgl_buffer_sub_data as NativeFn),
        ("deleteBuffer", webgl_delete_buffer as NativeFn),
        (
            "vertexAttribPointer",
            webgl_vertex_attrib_pointer as NativeFn,
        ),
        ("getAttribLocation", webgl_get_attrib_location as NativeFn),
    ] {
        init.function(native(function), js_string!(name), 8);
    }
    let object = init.build();
    let getter = accessor_fn(context, webgl_vertices_get);
    let setter = accessor_fn(context, webgl_vertices_set);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(getter))
        .set(JsValue::from(setter))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = object.define_property_or_throw(js_string!("vertices"), descriptor, context);
    object
}

/// WebGL 1.0 §5.14.
pub(crate) fn webgl_clear_color(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let mut channels = [0.0_f32; 4];
    for (index, channel) in channels.iter_mut().enumerate() {
        *channel = canvas_number(args, index, context)? as f32;
    }
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_clear_color(channels[0], channels[1], channels[2], channels[3]);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §5.14.
pub(crate) fn webgl_clear(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let mask = canvas_number(args, 0, context)? as u32;
    canvas_ops::with_surface(context, node, |surface| surface.webgl_clear(mask));
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §2.11.
pub(crate) fn webgl_viewport(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let mut values = [0_i32; 4];
    for (index, value) in values.iter_mut().enumerate() {
        *value = canvas_number(args, index, context)? as i32;
    }
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_viewport(values[0], values[1], values[2], values[3]);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §5.14: `DEPTH_TEST` is recorded; other capabilities are ignored.
pub(crate) fn webgl_enable(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let cap = canvas_number(args, 0, context)? as u32;
    canvas_ops::with_surface(context, node, |surface| {
        let _ = surface.webgl_enable(cap);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §2.12 + §6.2: `POINTS` reads `vertexAttribPointer`'s buffer when
/// present, otherwise the legacy `vertices` property.
pub(crate) fn webgl_draw_arrays(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let mode = canvas_number(args, 0, context)? as u32;
    let first = bounded_index(args.get(1), context);
    let count = bounded_index(args.get(2), context);
    canvas_ops::with_surface(context, node, |surface| {
        surface.webgl_draw_arrays(mode, first, count);
    });
    Ok(JsValue::undefined())
}

/// WebGL 1.0 §6.2: writes RGBA/UNSIGNED_BYTE pixels into a `Uint8Array`.
pub(crate) fn webgl_read_pixels(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 0, context)? as i64;
    let y = canvas_number(args, 1, context)? as i64;
    let width = bounded_index(args.get(2), context) as u32;
    let height = bounded_index(args.get(3), context) as u32;
    let format = canvas_number(args, 4, context)? as u32;
    let kind = canvas_number(args, 5, context)? as u32;
    let Some(destination) = args.get(6).and_then(JsValue::as_object) else {
        return Err(js_error("readPixels expects a Uint8Array destination"));
    };
    let pixels = canvas_ops::read_surface(context, node, |surface| {
        surface.webgl_read_pixels(x, y, width, height, format, kind)
    })
    .flatten();
    if let Some(pixels) = pixels {
        let _ = write_u8_array(&destination, &pixels, context);
    }
    Ok(JsValue::undefined())
}

/// Returns the current clip-space vertices as a flat JS array.
pub(crate) fn webgl_vertices_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let vertices = canvas_ops::read_surface(context, node, |surface| surface.webgl_vertices())
        .unwrap_or_default();
    let values = vertices
        .into_iter()
        .flat_map(|(x, y)| [JsValue::from(f64::from(x)), JsValue::from(f64::from(y))]);
    let array = JsArray::from_iter(values, context);
    Ok(JsValue::from(array))
}

/// Accepts a flat array/typed array of clip-space `[x, y, ...]` pairs.
pub(crate) fn webgl_vertices_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let Some(value) = args.first().and_then(JsValue::as_object) else {
        return Ok(JsValue::undefined());
    };
    let length = value
        .get(js_string!("length"), context)
        .ok()
        .and_then(|value| value.as_number())
        .unwrap_or(0.0) as usize;
    let length = length.min(webgl::MAX_DRAW_VERTICES.saturating_mul(2));
    let mut numbers = Vec::with_capacity(length);
    for index in 0..length {
        if let Some(number) = value
            .get(index, context)
            .ok()
            .and_then(|value| value.as_number())
        {
            numbers.push(number as f32);
        }
    }
    let vertices = numbers
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .collect();
    canvas_ops::with_surface(context, node, |surface| {
        surface.set_webgl_vertices(vertices)
    });
    Ok(JsValue::undefined())
}
