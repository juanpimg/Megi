//! Low-level helpers shared by the Canvas 2D binding modules.
//!
//! These resolve a native call's receiver and numeric arguments. They live
//! below both `canvas` and `canvas_paths` so neither binding module has to
//! import the other. Receiver resolution goes through the [`CanvasOps`] slot,
//! so this module never borrows the host or names a runtime type.
//! Spec: WHATWG HTML §4.12.5.1.

use super::*;
use crate::canvas::ops as canvas_ops;

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
    canvas_ops::resolve_handle(context, handle).ok_or_else(|| js_error("invalid DOM node handle"))
}

/// Resolves an `<img>`/`<canvas>` wrapper to a live node for `drawImage`.
pub(crate) fn canvas_element_node(value: &JsValue, context: &mut Context) -> Option<NodeId> {
    let handle = value
        .as_object()?
        .get(js_string!("__megi_node"), context)
        .ok()?
        .as_number()?;
    if handle < 0.0 || handle > f64::from(u32::MAX) || handle.fract() != 0.0 {
        return None;
    }
    canvas_ops::resolve_handle(context, handle as u32)
}

/// Reads a numeric Canvas 2D argument, erroring when it is missing.
pub(crate) fn canvas_number(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<f64> {
    args.get(index)
        .map(|value| value.to_number(context))
        .transpose()?
        .ok_or_else(|| js_error("canvas method: missing numeric argument"))
}
