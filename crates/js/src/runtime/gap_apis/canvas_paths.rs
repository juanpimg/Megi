//! Extended Canvas 2D bindings: paths, transforms, state stack, linear gradients, drawImage.
//!
//! Plan: docs/superpowers/plans/2026-11-24-engine-depth-plan.md §Task 6.
//! Spec: WHATWG HTML §4.12.5.1.

use super::canvas_support::{canvas_context_node, canvas_element_node, canvas_number};
use super::*;
use crate::canvas::PaintStyle;
use crate::canvas::ops as canvas_ops;

/// Registers extended 2D path, transform, gradient, and `drawImage` methods on the builder.
pub(crate) fn register_canvas_2d_methods(init: &mut ObjectInitializer<'_>) {
    for (name, arity, function) in [
        ("moveTo", 2usize, canvas_move_to as NativeFn),
        ("lineTo", 2, canvas_line_to as NativeFn),
        ("closePath", 0, canvas_close_path as NativeFn),
        ("quadraticCurveTo", 4, canvas_quadratic_curve_to as NativeFn),
        ("bezierCurveTo", 6, canvas_bezier_curve_to as NativeFn),
        ("rect", 4, canvas_rect as NativeFn),
        ("stroke", 0, canvas_stroke as NativeFn),
        ("save", 0, canvas_save as NativeFn),
        ("restore", 0, canvas_restore as NativeFn),
        ("translate", 2, canvas_translate as NativeFn),
        ("scale", 2, canvas_scale as NativeFn),
        ("rotate", 1, canvas_rotate as NativeFn),
        ("transform", 6, canvas_transform as NativeFn),
        ("setTransform", 6, canvas_set_transform as NativeFn),
        ("resetTransform", 0, canvas_reset_transform as NativeFn),
        ("drawImage", 9, canvas_draw_image as NativeFn),
        (
            "createLinearGradient",
            4,
            canvas_create_linear_gradient as NativeFn,
        ),
        (
            "createRadialGradient",
            6,
            canvas_create_radial_gradient as NativeFn,
        ),
    ] {
        init.function(native(function), js_string!(name), arity);
    }
}

/// Registers extended Canvas 2D property descriptors (`lineWidth`).
pub(crate) fn register_canvas_2d_accessors(
    context_object: &boa_engine::JsObject,
    context: &mut Context,
) {
    let get_lw = accessor_fn(context, canvas_line_width_get);
    let set_lw = accessor_fn(context, canvas_line_width_set);
    let descriptor = PropertyDescriptor::builder()
        .get(JsValue::from(get_lw))
        .set(JsValue::from(set_lw))
        .enumerable(true)
        .configurable(true)
        .build();
    let _ = context_object.define_property_or_throw(js_string!("lineWidth"), descriptor, context);
}

/// Applies either a CSS color string or a `CanvasGradient` object to `fillStyle`/`strokeStyle`.
pub(crate) fn set_paint_style(
    node: NodeId,
    arg: Option<&JsValue>,
    is_fill: bool,
    context: &mut Context,
) -> JsResult<()> {
    let Some(val) = arg else {
        return Ok(());
    };
    if let Some(obj) = val.as_object()
        && let Ok(grad_val) = obj.get(js_string!("__megi_gradient_id"), context)
        && let Some(grad_num) = grad_val.as_number()
        && grad_num.is_finite()
        && grad_num >= 0.0
    {
        let grad_id = grad_num as usize;
        canvas_ops::with_surface(context, node, |s| {
            s.set_paint(is_fill, PaintStyle::Gradient(grad_id));
        });
        return Ok(());
    }
    let text = val
        .to_string(context)
        .map(|s| s.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))?;
    if let Some(color) = crate::canvas::Rgba::parse(&text) {
        canvas_ops::with_surface(context, node, |s| {
            s.set_paint(is_fill, PaintStyle::Color(color));
        });
    }
    Ok(())
}

fn canvas_move_to(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 0, context)?;
    let y = canvas_number(args, 1, context)?;
    canvas_ops::with_surface(context, node, |s| s.move_to(x, y));
    Ok(JsValue::undefined())
}

fn canvas_line_to(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 0, context)?;
    let y = canvas_number(args, 1, context)?;
    canvas_ops::with_surface(context, node, |s| s.line_to(x, y));
    Ok(JsValue::undefined())
}

fn canvas_close_path(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    canvas_ops::with_surface(context, node, |s| s.close_path());
    Ok(JsValue::undefined())
}

fn canvas_quadratic_curve_to(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let cpx = canvas_number(args, 0, context)?;
    let cpy = canvas_number(args, 1, context)?;
    let x = canvas_number(args, 2, context)?;
    let y = canvas_number(args, 3, context)?;
    canvas_ops::with_surface(context, node, |s| s.quadratic_curve_to(cpx, cpy, x, y));
    Ok(JsValue::undefined())
}

fn canvas_bezier_curve_to(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let cp1x = canvas_number(args, 0, context)?;
    let cp1y = canvas_number(args, 1, context)?;
    let cp2x = canvas_number(args, 2, context)?;
    let cp2y = canvas_number(args, 3, context)?;
    let x = canvas_number(args, 4, context)?;
    let y = canvas_number(args, 5, context)?;
    canvas_ops::with_surface(context, node, |s| {
        s.bezier_curve_to(cp1x, cp1y, cp2x, cp2y, x, y)
    });
    Ok(JsValue::undefined())
}

fn canvas_rect(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x = canvas_number(args, 0, context)?;
    let y = canvas_number(args, 1, context)?;
    let w = canvas_number(args, 2, context)?;
    let h = canvas_number(args, 3, context)?;
    canvas_ops::with_surface(context, node, |s| s.rect_path(x, y, w, h));
    Ok(JsValue::undefined())
}

fn canvas_stroke(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    canvas_ops::with_surface(context, node, |s| s.stroke_path());
    Ok(JsValue::undefined())
}

fn canvas_save(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    canvas_ops::with_surface(context, node, |s| s.save());
    Ok(JsValue::undefined())
}

fn canvas_restore(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    canvas_ops::with_surface(context, node, |s| s.restore());
    Ok(JsValue::undefined())
}

fn canvas_translate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let tx = canvas_number(args, 0, context)?;
    let ty = canvas_number(args, 1, context)?;
    canvas_ops::with_surface(context, node, |s| s.translate(tx, ty));
    Ok(JsValue::undefined())
}

fn canvas_scale(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let sx = canvas_number(args, 0, context)?;
    let sy = canvas_number(args, 1, context)?;
    canvas_ops::with_surface(context, node, |s| s.scale(sx, sy));
    Ok(JsValue::undefined())
}

fn canvas_rotate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let angle = canvas_number(args, 0, context)?;
    canvas_ops::with_surface(context, node, |s| s.rotate(angle));
    Ok(JsValue::undefined())
}

fn canvas_transform(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let a = canvas_number(args, 0, context)?;
    let b = canvas_number(args, 1, context)?;
    let c = canvas_number(args, 2, context)?;
    let d = canvas_number(args, 3, context)?;
    let e = canvas_number(args, 4, context)?;
    let f = canvas_number(args, 5, context)?;
    canvas_ops::with_surface(context, node, |s| s.concat_transform(a, b, c, d, e, f));
    Ok(JsValue::undefined())
}

fn canvas_set_transform(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let a = canvas_number(args, 0, context)?;
    let b = canvas_number(args, 1, context)?;
    let c = canvas_number(args, 2, context)?;
    let d = canvas_number(args, 3, context)?;
    let e = canvas_number(args, 4, context)?;
    let f = canvas_number(args, 5, context)?;
    canvas_ops::with_surface(context, node, |s| s.set_transform(a, b, c, d, e, f));
    Ok(JsValue::undefined())
}

fn canvas_reset_transform(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    canvas_ops::with_surface(context, node, |s| s.reset_transform());
    Ok(JsValue::undefined())
}

fn canvas_line_width_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let lw = canvas_ops::read_surface(context, node, |s| s.line_width).unwrap_or(1.0);
    Ok(JsValue::from(lw))
}

fn canvas_line_width_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let Some(raw) = args.first().and_then(|v| v.as_number()) else {
        return Ok(JsValue::undefined());
    };
    canvas_ops::with_surface(context, node, |s| s.set_line_width(raw));
    Ok(JsValue::undefined())
}

fn canvas_create_linear_gradient(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x0 = canvas_number(args, 0, context)?;
    let y0 = canvas_number(args, 1, context)?;
    let x1 = canvas_number(args, 2, context)?;
    let y1 = canvas_number(args, 3, context)?;
    let handle = canvas_ops::handle_for(context, node)
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    let grad_id =
        canvas_ops::surface_value(context, node, |s| s.create_linear_gradient(x0, y0, x1, y1))
            .flatten()
            .ok_or_else(|| js_error("canvas gradient limit or invalid coordinates"))?;
    canvas_gradient_object(handle, grad_id, context)
}

/// Builds the JS `CanvasGradient` wrapper used by both gradient constructors.
fn canvas_gradient_object(handle: u32, grad_id: usize, context: &mut Context) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__megi_canvas_node"),
        JsValue::from(handle),
        Attribute::all(),
    );
    init.property(
        js_string!("__megi_gradient_id"),
        JsValue::from(grad_id as u32),
        Attribute::all(),
    );
    init.function(
        native(canvas_gradient_add_color_stop),
        js_string!("addColorStop"),
        2,
    );
    Ok(init.build().into())
}

/// `createRadialGradient` (WHATWG HTML §4.12.5.1.5): a gradient between two circles.
fn canvas_create_radial_gradient(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let x0 = canvas_number(args, 0, context)?;
    let y0 = canvas_number(args, 1, context)?;
    let r0 = canvas_number(args, 2, context)?;
    let x1 = canvas_number(args, 3, context)?;
    let y1 = canvas_number(args, 4, context)?;
    let r1 = canvas_number(args, 5, context)?;
    // WHATWG HTML §4.12.5.1.5: radii must be finite and non-negative.
    if r0 < 0.0 || r1 < 0.0 || !r0.is_finite() || !r1.is_finite() {
        return Err(js_error("createRadialGradient expects non-negative radii"));
    }
    let handle = canvas_ops::handle_for(context, node)
        .ok_or_else(|| js_error("no active JavaScript host"))?;
    let grad_id = canvas_ops::surface_value(context, node, |s| {
        s.create_radial_gradient(x0, y0, r0, x1, y1, r1)
    })
    .flatten()
    .ok_or_else(|| js_error("canvas gradient limit or invalid coordinates"))?;
    canvas_gradient_object(handle, grad_id, context)
}

fn canvas_gradient_add_color_stop(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = canvas_context_node(this, context)?;
    let obj = this
        .as_object()
        .ok_or_else(|| js_error("invalid gradient receiver"))?;
    let grad_id = obj
        .get(js_string!("__megi_gradient_id"), context)
        .ok()
        .and_then(|v| v.as_number())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| js_error("invalid gradient id"))? as usize;
    let offset = canvas_number(args, 0, context)?;
    let color_str = arg_string(args, 1, context)?;
    let Some(color) = crate::canvas::Rgba::parse(&color_str) else {
        return Err(js_error("invalid gradient color stop"));
    };
    canvas_ops::with_surface(context, node, |s| {
        let _ = s.add_gradient_stop(grad_id, offset, color);
    });
    Ok(JsValue::undefined())
}

fn canvas_draw_image(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let dst_node = canvas_context_node(this, context)?;
    let Some(image_arg) = args.first() else {
        return Ok(JsValue::undefined());
    };
    let Some(src_node) = canvas_element_node(image_arg, context) else {
        return Ok(JsValue::undefined());
    };
    let Some(src_snapshot) = canvas_ops::snapshot(context, src_node) else {
        return Ok(JsValue::undefined());
    };
    let sw_full = f64::from(src_snapshot.width);
    let sh_full = f64::from(src_snapshot.height);
    let (sx, sy, sw, sh, dx, dy, dw, dh) = if args.len() >= 9 {
        (
            canvas_number(args, 1, context)?,
            canvas_number(args, 2, context)?,
            canvas_number(args, 3, context)?,
            canvas_number(args, 4, context)?,
            canvas_number(args, 5, context)?,
            canvas_number(args, 6, context)?,
            canvas_number(args, 7, context)?,
            canvas_number(args, 8, context)?,
        )
    } else if args.len() >= 5 {
        (
            0.0,
            0.0,
            sw_full,
            sh_full,
            canvas_number(args, 1, context)?,
            canvas_number(args, 2, context)?,
            canvas_number(args, 3, context)?,
            canvas_number(args, 4, context)?,
        )
    } else {
        (
            0.0,
            0.0,
            sw_full,
            sh_full,
            canvas_number(args, 1, context)?,
            canvas_number(args, 2, context)?,
            sw_full,
            sh_full,
        )
    };
    canvas_ops::with_surface(context, dst_node, |dst| {
        dst.draw_surface(&src_snapshot, sx, sy, sw, sh, dx, dy, dw, dh);
    });
    Ok(JsValue::undefined())
}
