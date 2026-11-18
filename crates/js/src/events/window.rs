use super::*;

pub(crate) fn viewport_info(context: &mut Context) -> ViewportInfo {
    // Providers live in an immutable slot independent of the host cell, so cloning
    // the Arc lets us call it with no active host borrow (it may re-enter JS).
    let provider = provider_or_mirror(
        context,
        |slot| slot.viewport.clone(),
        |host| host.viewport_provider.clone(),
    );
    let (scroll_x, scroll_y) = {
        let Ok(host) = host_cell(context) else {
            return ViewportInfo::default();
        };
        (host.scroll_x, host.scroll_y)
    };
    let mut info = provider
        .as_ref()
        .map(|provider| provider())
        .unwrap_or_default();
    info.scroll_x = scroll_x;
    info.scroll_y = scroll_y;
    info
}

pub(crate) fn window_inner_width(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(f64::from(viewport_info(context).width)))
}

pub(crate) fn window_inner_height(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(f64::from(viewport_info(context).height)))
}

pub(crate) fn window_device_pixel_ratio(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info(context).device_pixel_ratio))
}

pub(crate) fn window_page_x(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info(context).scroll_x))
}

pub(crate) fn window_page_y(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(viewport_info(context).scroll_y))
}

pub(crate) fn match_media(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let query = arg_string(args, 0, context)?;
    // Clone the Arc from the immutable provider slot so it runs with no host borrow.
    let provider = provider_or_mirror(
        context,
        |slot| slot.media.clone(),
        |host| host.media_provider.clone(),
    );
    let matches = provider
        .as_ref()
        .map(|provider| provider(&query))
        .unwrap_or(false);
    let add_listener = accessor_fn(context, noop_listener);
    let remove_listener = accessor_fn(context, noop_listener);
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("__megi_media_query"), true, Attribute::all())
        .property(
            js_string!("media"),
            js_string!(query.clone()),
            Attribute::all(),
        )
        .property(js_string!("matches"), matches, Attribute::all())
        .property(js_string!("onchange"), JsValue::null(), Attribute::all())
        .function(native(noop_listener), js_string!("addListener"), 1)
        .function(native(noop_listener), js_string!("removeListener"), 1)
        .function(native(noop_listener), js_string!("addEventListener"), 2)
        .function(native(noop_listener), js_string!("removeEventListener"), 2);
    let _ = (add_listener, remove_listener);
    Ok(JsValue::from(init.build()))
}

/// `MediaQueryList` listener registration is accepted but does not fire `change` events
/// until the host re-evaluates the page (documented limitation, like a one-shot render).
pub(crate) fn noop_listener(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

pub(crate) fn scroll_coordinates(
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<Option<(f64, f64)>> {
    if let Some(options) = args.first().and_then(|value| value.as_object()) {
        let left = prop(&options, "left", context)
            .ok()
            .and_then(|value| value.as_number())
            .unwrap_or(0.0);
        let top = prop(&options, "top", context)
            .ok()
            .and_then(|value| value.as_number())
            .unwrap_or(0.0);
        return Ok(Some((left, top)));
    }
    let x = args.first().and_then(JsValue::as_number);
    let y = args.get(1).and_then(JsValue::as_number);
    match (x, y) {
        (Some(x), Some(y)) => Ok(Some((x, y))),
        (Some(x), None) => Ok(Some((x, 0.0))),
        _ => Ok(None),
    }
}

pub(crate) fn scroll_to(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some((x, y)) = scroll_coordinates(args, context)? {
        let mut host = host_cell(context)?;
        host.scroll_x = x.max(0.0);
        host.scroll_y = y.max(0.0);
    }
    Ok(JsValue::undefined())
}

pub(crate) fn scroll_by(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if let Some((x, y)) = scroll_coordinates(args, context)? {
        let mut host = host_cell(context)?;
        host.scroll_x = (host.scroll_x + x).max(0.0);
        host.scroll_y = (host.scroll_y + y).max(0.0);
    }
    Ok(JsValue::undefined())
}

/// Reports an uncaught script exception: invokes `window.onerror`/`document.onerror`
/// (when installed) with `(message, source, lineno, colno, error)` and records the error
/// in the console capture buffer.
pub(crate) fn report_uncaught(
    context: &mut Context,
    message: &str,
    source: &str,
) -> Result<(), CrateJsError> {
    let window = JsValue::from(context.global_object());
    let document = global_get(context, "document")
        .map_err(|error| CrateJsError::Internal(error.to_string()))?;
    for (target, this) in [(TargetKey::Window, window), (TargetKey::Document, document)] {
        let slot = {
            let host =
                host_ref(context).map_err(|error| CrateJsError::Runtime(error.to_string()))?;
            host.inline_handlers
                .iter()
                .find(|handler| handler.target == target && handler.event_type == "error")
                .map(|handler| handler.slot)
        };
        if let Some(slot) = slot {
            let callback = array_get(context, "__megi_inline_handlers", u64::from(slot))
                .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
            if let Some(callback) = callback.as_object() {
                let args = [
                    JsValue::from(js_string!(message)),
                    JsValue::from(js_string!(source)),
                    JsValue::from(0.0),
                    JsValue::from(0.0),
                    JsValue::from(js_string!(message)),
                ];
                let _ = callback.call(&this, &args, context);
            }
        }
    }
    host_cell(context)
        .map_err(|error| CrateJsError::Runtime(error.to_string()))?
        .push_console(crate::host::ConsoleEntry {
            level: "error".to_string(),
            message: format!("Uncaught {message}"),
        });
    Ok(())
}

// ---------------------------------------------------------------------------
// getComputedStyle
// ---------------------------------------------------------------------------
