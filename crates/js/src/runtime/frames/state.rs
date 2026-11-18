use super::*;

// ---------------------------------------------------------------------------
// Nested browsing contexts (`<iframe>`, sandbox, same-origin, postMessage, #1544)
// ---------------------------------------------------------------------------

pub(crate) const MAX_FRAME_DEPTH: usize = 4;
pub(crate) const MAX_FRAMES_PER_PAGE: usize = 16;
pub(crate) const MAX_FRAME_MESSAGES: usize = 256;

/// Live child browsing context owned by a parent `Host` for one `<iframe>` element.
pub(crate) struct IframeState {
    pub node_id: NodeId,
    pub runtime: JsRuntime,
    pub url: String,
    pub same_origin: bool,
    pub sandbox: SandboxFlags,
    pub to_child: VecDeque<(String, String)>,
}

pub(crate) fn invalidate_iframe_state(host: &mut Host, node_id: NodeId) {
    if let Some(idx) = host.frame_by_node.remove(&node_id)
        && let Some(slot) = host.frames.get_mut(idx as usize)
    {
        *slot = None;
    }
}

pub(crate) fn target_origin_matches(
    target_origin: &str,
    receiver_origin: &str,
    sender_origin: &str,
) -> bool {
    let trimmed = target_origin.trim();
    if trimmed == "*" {
        return true;
    }
    if trimmed == "/" {
        return receiver_origin == sender_origin;
    }
    let normalized = if trimmed.contains("://") {
        crate::host::origin_for_url(trimmed, false)
    } else {
        trimmed.to_string()
    };
    normalized == receiver_origin && receiver_origin != "null"
}

pub(crate) fn stringify_for_message(value: &JsValue, context: &mut Context) -> JsResult<String> {
    if value.is_undefined() {
        return Ok("null".to_string());
    }
    let json_val = context
        .global_object()
        .get(js_string!("JSON"), context)
        .map_err(|e| js_error(e.to_string()))?;
    let json_obj = json_val
        .as_object()
        .ok_or_else(|| js_error("JSON object missing"))?;
    let stringify_fn = json_obj
        .get(js_string!("stringify"), context)
        .map_err(|e| js_error(e.to_string()))?
        .as_callable()
        .ok_or_else(|| js_error("JSON.stringify missing"))?;
    let result = stringify_fn.call(&json_val, std::slice::from_ref(value), context)?;
    if result.is_undefined() {
        Ok("null".to_string())
    } else {
        result
            .to_string(context)
            .map(|s| s.to_std_string_escaped())
            .map_err(|e| js_error(e.to_string()))
    }
}

pub(crate) enum BridgeArg {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    FrameRef(u32),
    Json(String),
}

pub(crate) enum BridgeResult {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Ref { ref_id: u32, is_fn: bool },
}

pub(crate) fn bridge_arg_from_parent(
    value: &JsValue,
    frame_id: u32,
    context: &mut Context,
) -> BridgeArg {
    if value.is_undefined() {
        return BridgeArg::Undefined;
    }
    if value.is_null() {
        return BridgeArg::Null;
    }
    if let Some(b) = value.as_boolean() {
        return BridgeArg::Bool(b);
    }
    if let Some(n) = value.as_number() {
        return BridgeArg::Number(n);
    }
    if let Some(s) = value.as_string() {
        return BridgeArg::String(s.to_std_string_escaped());
    }
    if let Some(obj) = value.as_object()
        && let Ok(fid_val) = obj.get(js_string!("__megi_frame_id"), context)
        && let Some(fid) = fid_val.as_number()
        && fid as u32 == frame_id
        && let Ok(rid_val) = obj.get(js_string!("__megi_ref_id"), context)
        && let Some(rid) = rid_val.as_number()
    {
        return BridgeArg::FrameRef(rid as u32);
    }
    let json = stringify_for_message(value, context).unwrap_or_else(|_| "null".to_string());
    BridgeArg::Json(json)
}

pub(crate) fn bridge_arg_to_child(arg: BridgeArg, context: &mut Context) -> JsResult<JsValue> {
    match arg {
        BridgeArg::Undefined => Ok(JsValue::undefined()),
        BridgeArg::Null => Ok(JsValue::null()),
        BridgeArg::Bool(b) => Ok(JsValue::from(b)),
        BridgeArg::Number(n) => Ok(JsValue::from(n)),
        BridgeArg::String(s) => Ok(JsValue::from(js_string!(s))),
        BridgeArg::FrameRef(ref_id) => {
            let refs = context
                .global_object()
                .get(js_string!("__megi_bridge_refs"), context)
                .map_err(|e| js_error(e.to_string()))?;
            let refs_obj = refs
                .as_object()
                .ok_or_else(|| js_error("missing bridge refs"))?;
            refs_obj
                .get(ref_id, context)
                .map_err(|e| js_error(e.to_string()))
        }
        BridgeArg::Json(json) => {
            let json_val = context
                .global_object()
                .get(js_string!("JSON"), context)
                .map_err(|e| js_error(e.to_string()))?;
            let json_obj = json_val
                .as_object()
                .ok_or_else(|| js_error("JSON object missing"))?;
            let parse_fn = json_obj
                .get(js_string!("parse"), context)
                .map_err(|e| js_error(e.to_string()))?
                .as_callable()
                .ok_or_else(|| js_error("JSON.parse missing"))?;
            parse_fn.call(&json_val, &[JsValue::from(js_string!(json))], context)
        }
    }
}

pub(crate) fn bridge_encode_child_value(
    value: JsValue,
    context: &mut Context,
) -> JsResult<BridgeResult> {
    if value.is_undefined() {
        return Ok(BridgeResult::Undefined);
    }
    if value.is_null() {
        return Ok(BridgeResult::Null);
    }
    if let Some(b) = value.as_boolean() {
        return Ok(BridgeResult::Bool(b));
    }
    if let Some(n) = value.as_number() {
        return Ok(BridgeResult::Number(n));
    }
    if let Some(s) = value.as_string() {
        return Ok(BridgeResult::String(s.to_std_string_escaped()));
    }
    let Some(obj) = value.as_object() else {
        let text = value
            .to_string(context)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        return Ok(BridgeResult::String(text));
    };
    let is_fn = obj.is_callable();
    let refs_val = context
        .global_object()
        .get(js_string!("__megi_bridge_refs"), context)
        .map_err(|e| js_error(e.to_string()))?;
    let refs = refs_val
        .as_object()
        .ok_or_else(|| js_error("missing bridge refs"))?;
    let len = refs
        .get(js_string!("length"), context)
        .ok()
        .and_then(|v| v.as_number())
        .unwrap_or(0.0) as u32;
    for idx in 0..len {
        if let Ok(existing) = refs.get(idx, context)
            && existing.strict_equals(&value)
        {
            return Ok(BridgeResult::Ref { ref_id: idx, is_fn });
        }
    }
    let _ = refs.set(len, value, false, context);
    Ok(BridgeResult::Ref { ref_id: len, is_fn })
}

pub(crate) fn bridge_decode_to_parent(
    result: BridgeResult,
    frame_id: u32,
    owner_ref_id: Option<u32>,
    context: &mut Context,
) -> JsResult<JsValue> {
    match result {
        BridgeResult::Undefined => Ok(JsValue::undefined()),
        BridgeResult::Null => Ok(JsValue::null()),
        BridgeResult::Bool(b) => Ok(JsValue::from(b)),
        BridgeResult::Number(n) => Ok(JsValue::from(n)),
        BridgeResult::String(s) => Ok(JsValue::from(js_string!(s))),
        BridgeResult::Ref { ref_id, is_fn } => {
            let wrap = context
                .global_object()
                .get(js_string!("__megiWrapFrameObj"), context)
                .map_err(|e| js_error(e.to_string()))?;
            let wrap_fn = wrap
                .as_callable()
                .ok_or_else(|| js_error("__megiWrapFrameObj missing"))?;
            let recv = owner_ref_id
                .map(|id| JsValue::from(f64::from(id)))
                .unwrap_or_else(JsValue::undefined);
            wrap_fn.call(
                &JsValue::undefined(),
                &[
                    JsValue::from(f64::from(frame_id)),
                    JsValue::from(f64::from(ref_id)),
                    JsValue::from(is_fn),
                    recv,
                ],
                context,
            )
        }
    }
}

pub(crate) fn get_iframe_rc(
    frame_id: u32,
    context: &mut Context,
) -> Option<Rc<RefCell<IframeState>>> {
    with_context_host(context, |host| {
        host.frames
            .get(frame_id as usize)
            .and_then(|slot| slot.as_ref().map(Rc::clone))
    })
    .flatten()
}
