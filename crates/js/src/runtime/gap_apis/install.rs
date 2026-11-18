use super::*;

/// Installs small standard-library gap APIs (URL/SearchParams, base64, structuredClone,
/// queueMicrotask, requestIdleCallback, MouseEvent/PointerEvent and Node constants).
// JS polyfills/globals injected as a raw string; kept as .js data files so
// each Rust source file stays within the size budget.
pub(crate) const PRELUDE: &str =
    concat!(include_str!("prelude_a.js"), include_str!("prelude_b.js"),);

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
