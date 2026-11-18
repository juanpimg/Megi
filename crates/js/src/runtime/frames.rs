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

pub(crate) fn frame_bridge_get(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let frame_id = args.first().and_then(JsValue::as_number).unwrap_or(-1.0) as u32;
    let ref_id = args.get(1).and_then(JsValue::as_number).unwrap_or(0.0) as u32;
    let prop = arg_string(args, 2, context)?;
    let state_rc = get_iframe_rc(frame_id, context)
        .ok_or_else(|| js_error("TypeError: detached iframe browsing context"))?;
    let encoded = {
        let mut state = state_rc.borrow_mut();
        if !state.same_origin || host_opaque_origin(&state.runtime) {
            return Err(js_error(
                "SecurityError: Blocked a frame from accessing a cross-origin frame.",
            ));
        }
        let child_ctx = &mut state.runtime.context;
        let obj = bridge_arg_to_child(BridgeArg::FrameRef(ref_id), child_ctx)?
            .as_object()
            .ok_or_else(|| js_error("TypeError: invalid bridged frame reference"))?;
        let val = obj
            .get(js_string!(prop), child_ctx)
            .map_err(|e| js_error(e.to_string()))?;
        bridge_encode_child_value(val, child_ctx)?
    };
    bridge_decode_to_parent(encoded, frame_id, Some(ref_id), context)
}

pub(crate) fn frame_bridge_set(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let frame_id = args.first().and_then(JsValue::as_number).unwrap_or(-1.0) as u32;
    let ref_id = args.get(1).and_then(JsValue::as_number).unwrap_or(0.0) as u32;
    let prop = arg_string(args, 2, context)?;
    let val_arg = bridge_arg_from_parent(
        args.get(3).unwrap_or(&JsValue::undefined()),
        frame_id,
        context,
    );
    let state_rc = get_iframe_rc(frame_id, context)
        .ok_or_else(|| js_error("TypeError: detached iframe browsing context"))?;
    let mut state = state_rc.borrow_mut();
    if !state.same_origin || host_opaque_origin(&state.runtime) {
        return Err(js_error(
            "SecurityError: Blocked a frame from accessing a cross-origin frame.",
        ));
    }
    let child_ctx = &mut state.runtime.context;
    let obj = bridge_arg_to_child(BridgeArg::FrameRef(ref_id), child_ctx)?
        .as_object()
        .ok_or_else(|| js_error("TypeError: invalid bridged frame reference"))?;
    let child_val = bridge_arg_to_child(val_arg, child_ctx)?;
    obj.set(js_string!(prop), child_val, false, child_ctx)
        .map_err(|e| js_error(e.to_string()))?;
    Ok(JsValue::from(true))
}

pub(crate) fn frame_bridge_has(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let frame_id = args.first().and_then(JsValue::as_number).unwrap_or(-1.0) as u32;
    let ref_id = args.get(1).and_then(JsValue::as_number).unwrap_or(0.0) as u32;
    let prop = arg_string(args, 2, context)?;
    let state_rc = get_iframe_rc(frame_id, context)
        .ok_or_else(|| js_error("TypeError: detached iframe browsing context"))?;
    let state = state_rc.borrow();
    if !state.same_origin || host_opaque_origin(&state.runtime) {
        return Ok(JsValue::from(false));
    }
    drop(state);
    let mut state = state_rc.borrow_mut();
    let child_ctx = &mut state.runtime.context;
    let obj = bridge_arg_to_child(BridgeArg::FrameRef(ref_id), child_ctx)?
        .as_object()
        .ok_or_else(|| js_error("TypeError: invalid bridged frame reference"))?;
    let has = obj
        .has_property(js_string!(prop), child_ctx)
        .map_err(|e| js_error(e.to_string()))?;
    Ok(JsValue::from(has))
}

pub(crate) fn frame_bridge_call(
    _t: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let frame_id = args.first().and_then(JsValue::as_number).unwrap_or(-1.0) as u32;
    let fn_ref_id = args.get(1).and_then(JsValue::as_number).unwrap_or(0.0) as u32;
    let recv_ref_id = args.get(2).and_then(JsValue::as_number).map(|n| n as u32);
    let mut call_args = Vec::new();
    if let Some(arr_obj) = args.get(3).and_then(JsValue::as_object) {
        let len = arr_obj
            .get(js_string!("length"), context)
            .ok()
            .and_then(|v| v.as_number())
            .unwrap_or(0.0) as usize;
        for i in 0..len {
            let item = arr_obj
                .get(i, context)
                .unwrap_or_else(|_| JsValue::undefined());
            call_args.push(bridge_arg_from_parent(&item, frame_id, context));
        }
    }
    let state_rc = get_iframe_rc(frame_id, context)
        .ok_or_else(|| js_error("TypeError: detached iframe browsing context"))?;
    let encoded = {
        let mut state = state_rc.borrow_mut();
        if !state.same_origin || host_opaque_origin(&state.runtime) {
            return Err(js_error(
                "SecurityError: Blocked a frame from accessing a cross-origin frame.",
            ));
        }
        let child_ctx = &mut state.runtime.context;
        let func_val = bridge_arg_to_child(BridgeArg::FrameRef(fn_ref_id), child_ctx)?;
        let func_obj = func_val
            .as_callable()
            .ok_or_else(|| js_error("TypeError: bridged property is not a function"))?;
        let this_val = match recv_ref_id {
            Some(rid) => bridge_arg_to_child(BridgeArg::FrameRef(rid), child_ctx)?,
            None => JsValue::undefined(),
        };
        let mut child_args = Vec::with_capacity(call_args.len());
        for a in call_args {
            child_args.push(bridge_arg_to_child(a, child_ctx)?);
        }
        let res = func_obj
            .call(&this_val, &child_args, child_ctx)
            .map_err(|e| js_error(e.to_string()))?;
        bridge_encode_child_value(res, child_ctx)?
    };
    bridge_decode_to_parent(encoded, frame_id, None, context)
}

/// Reads a nested realm's sandbox flag from its context-installed host cell.
pub(crate) fn host_opaque_origin(runtime: &JsRuntime) -> bool {
    host_ref(&runtime.context)
        .map(|host| host.opaque_origin)
        .unwrap_or(false)
}

pub(crate) fn node_is_connected(doc: &Document, node_id: NodeId) -> bool {
    let root = doc.root();
    let mut cur = Some(node_id);
    let mut steps = 0usize;
    let limit = doc.len().saturating_add(1);
    while let Some(c) = cur {
        if c == root {
            return true;
        }
        steps += 1;
        if steps > limit {
            break;
        }
        cur = doc.try_node(c).and_then(|n| n.parent);
    }
    false
}

pub(crate) fn post_target_origin_arg(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<String> {
    let Some(val) = args.get(index) else {
        return Ok("/".to_string());
    };
    if val.is_undefined() {
        return Ok("/".to_string());
    }
    val.to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn ensure_iframe_state(
    node_id: NodeId,
    context: &mut Context,
) -> Result<(u32, Rc<RefCell<IframeState>>), JsError> {
    let existing = with_context_host(context, |host| {
        if let Some(&idx) = host.frame_by_node.get(&node_id)
            && let Some(Some(rc)) = host.frames.get(idx as usize)
        {
            return Some((idx, Rc::clone(rc)));
        }
        None
    })
    .flatten();
    if let Some(found) = existing {
        return Ok(found);
    }

    let (sandbox, srcdoc, parent_url, parent_depth, opaque, parent_origin, resolved_url, loader) =
        with_context_host(context, |host| {
            if host.frame_depth >= MAX_FRAME_DEPTH {
                return Err("maximum iframe depth reached".to_string());
            }
            let active_count = host.frames.iter().filter(|s| s.is_some()).count();
            if active_count >= MAX_FRAMES_PER_PAGE {
                return Err("maximum iframe count per page reached".to_string());
            }
            let el = host
                .document
                .try_node(node_id)
                .and_then(|n| n.as_element())
                .ok_or_else(|| "node is not an element".to_string())?;
            if !el.local_name.eq_ignore_ascii_case("iframe") {
                return Err("node is not an iframe".to_string());
            }
            let sandbox = el
                .get_attribute("sandbox")
                .map(SandboxFlags::from_attribute)
                .unwrap_or_default();
            let srcdoc = el.get_attribute("srcdoc").map(str::to_owned);
            let raw_src = el
                .get_attribute("src")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let parent_url = host.url.clone();
            let parent_depth = host.frame_depth;
            let opaque = !sandbox.allow_same_origin;
            let resolved_url: String = if srcdoc.is_some() {
                parent_url.clone()
            } else if let Some(src) = raw_src.as_deref() {
                if src.eq_ignore_ascii_case("about:blank") {
                    "about:blank".to_string()
                } else {
                    host.resolve_url(src)
                }
            } else {
                "about:blank".to_string()
            };
            let parent_origin = crate::host::origin_for_url(&parent_url, host.opaque_origin);
            let loader = host.frame_loader.clone();
            Ok((
                sandbox,
                srcdoc,
                parent_url,
                parent_depth,
                opaque,
                parent_origin,
                resolved_url,
                loader,
            ))
        })
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;

    // Invoke the frame loader outside the host borrow: a loader may re-enter JS.
    // `&resolved_url` derefs the owned `String` to the `&str` the provider expects.
    let html_result = if let Some(doc_html) = srcdoc {
        Some((resolved_url.clone(), doc_html))
    } else if resolved_url == "about:blank" {
        Some((
            "about:blank".to_string(),
            "<html><head></head><body></body></html>".to_string(),
        ))
    } else if let Some(loader) = loader {
        loader(&resolved_url).ok()
    } else {
        None
    };
    let (final_url, html) = html_result.unwrap_or((
        resolved_url,
        "<html><head></head><body></body></html>".to_string(),
    ));
    let same_origin = crate::host::is_same_origin_url(&parent_url, &final_url, opaque);
    let parsed_doc =
        megi_html::parse_document_with_options(&html, false, &megi_html::ParseOptions::DEFAULT)
            .document;

    let inline_scripts: Vec<String> = if sandbox.allow_scripts {
        parsed_doc
            .get_elements_by_tag_name("script")
            .into_iter()
            .filter_map(|script_id| {
                let el = parsed_doc.try_node(script_id)?.as_element()?;
                if el.has_attribute("src") {
                    return None;
                }
                let ty = el
                    .get_attribute("type")
                    .map(|t| t.trim().to_ascii_lowercase())
                    .unwrap_or_default();
                if matches!(
                    ty.as_str(),
                    "" | "text/javascript" | "application/javascript"
                ) {
                    Some(parsed_doc.text_content(script_id))
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut child_runtime = JsRuntime::with_document(parsed_doc, final_url.clone())?;
    {
        let mut child_host =
            host_cell(&mut child_runtime.context).expect("JavaScript host installed in context");
        child_host.opaque_origin = opaque;
        child_host.allow_forms = sandbox.allow_forms;
        child_host.allow_popups = sandbox.allow_popups;
        child_host.frame_depth = parent_depth + 1;
        child_host.is_child_frame = true;
        child_host.parent_origin = Some(parent_origin);
        child_host.parent_same_origin = same_origin;
        child_host.parent_outbox = Some(VecDeque::new());
    }

    for script_src in inline_scripts {
        let _ = child_runtime.eval(&script_src, "iframe-inline");
        let _ = child_runtime.run_microtasks();
    }

    let state_rc = Rc::new(RefCell::new(IframeState {
        node_id,
        runtime: child_runtime,
        url: final_url,
        same_origin,
        sandbox,
        to_child: VecDeque::new(),
    }));

    let idx = with_context_host(context, |host| {
        if let Some(&idx) = host.frame_by_node.get(&node_id)
            && let Some(Some(_)) = host.frames.get(idx as usize)
        {
            return idx;
        }
        let idx = host.frames.len() as u32;
        host.frames.push(Some(Rc::clone(&state_rc)));
        host.frame_by_node.insert(node_id, idx);
        idx
    })
    .ok_or_else(|| js_error("no active JavaScript host"))?;

    Ok((idx, state_rc))
}

pub(crate) fn node_content_document_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let connected = with_context_host(context, |host| node_is_connected(&host.document, node_id))
        .unwrap_or(false);
    if !connected {
        return Ok(JsValue::null());
    }
    let Ok((frame_id, state_rc)) = ensure_iframe_state(node_id, context) else {
        return Ok(JsValue::null());
    };
    let allow = {
        let st = state_rc.borrow();
        st.same_origin && !host_opaque_origin(&st.runtime)
    };
    if !allow {
        return Ok(JsValue::null());
    }
    bridge_decode_to_parent(
        BridgeResult::Ref {
            ref_id: 0,
            is_fn: false,
        },
        frame_id,
        None,
        context,
    )
}

pub(crate) fn build_restricted_frame_window(
    context: &mut Context,
    frame_id: Option<u32>,
    is_parent_proxy: bool,
) -> JsResult<JsObject> {
    let mut init = ObjectInitializer::new(context);
    init.property(js_string!("closed"), false, Attribute::all());
    if let Some(fid) = frame_id {
        init.property(
            js_string!("__megi_xorigin_frame"),
            f64::from(fid),
            Attribute::default(),
        );
        init.function(
            native(cross_origin_post_message),
            js_string!("postMessage"),
            2,
        );
    } else if is_parent_proxy {
        init.property(
            js_string!("__megi_parent_proxy"),
            true,
            Attribute::default(),
        );
        init.function(native(parent_post_message), js_string!("postMessage"), 2);
    }
    let obj = init.build();
    for blocked in ["document", "location"] {
        let getter = accessor_fn(context, cross_origin_security_error);
        let desc = PropertyDescriptor::builder()
            .get(JsValue::from(getter))
            .enumerable(false)
            .configurable(true)
            .build();
        let _ = obj.define_property_or_throw(js_string!(blocked), desc, context);
    }
    Ok(obj)
}

pub(crate) fn cross_origin_security_error(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Err(js_error(
        "SecurityError: Blocked a frame with origin from accessing a cross-origin frame.",
    ))
}

pub(crate) fn enqueue_parent_to_child_message(
    frame_id: u32,
    message: &JsValue,
    target_origin: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let payload = stringify_for_message(message, context)?;
    let sender_origin = with_context_host(context, |host| {
        crate::host::origin_for_url(&host.url, host.opaque_origin)
    })
    .unwrap_or_else(|| "null".to_string());
    let Some(state_rc) = get_iframe_rc(frame_id, context) else {
        return Ok(JsValue::undefined());
    };
    let mut state = state_rc.borrow_mut();
    let receiver_origin =
        crate::host::origin_for_url(&state.url, host_opaque_origin(&state.runtime));
    if target_origin_matches(target_origin, &receiver_origin, &sender_origin)
        && state.to_child.len() < MAX_FRAME_MESSAGES
    {
        state.to_child.push_back((payload, sender_origin));
    }
    Ok(JsValue::undefined())
}

pub(crate) fn frame_bridge_post_message(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let frame_id = args.first().and_then(JsValue::as_number).unwrap_or(-1.0) as u32;
    let undefined = JsValue::undefined();
    let message = args.get(1).unwrap_or(&undefined);
    let target_origin = post_target_origin_arg(args, 2, context)?;
    enqueue_parent_to_child_message(frame_id, message, &target_origin, context)
}

pub(crate) fn cross_origin_post_message(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let obj = this
        .as_object()
        .ok_or_else(|| js_error("TypeError: Illegal invocation"))?;
    let frame_id = obj
        .get(js_string!("__megi_xorigin_frame"), context)
        .ok()
        .and_then(|v| v.as_number())
        .ok_or_else(|| js_error("TypeError: missing target frame"))? as u32;
    let undefined = JsValue::undefined();
    let message = args.first().unwrap_or(&undefined);
    let target_origin = post_target_origin_arg(args, 1, context)?;
    enqueue_parent_to_child_message(frame_id, message, &target_origin, context)
}

pub(crate) fn parent_post_message(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let payload = stringify_for_message(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let target_origin = post_target_origin_arg(args, 1, context)?;
    with_context_host(context, |host| {
        let child_origin = crate::host::origin_for_url(&host.url, host.opaque_origin);
        if let Some(outbox) = &mut host.parent_outbox
            && outbox.len() < MAX_FRAME_MESSAGES
        {
            outbox.push_back((payload, child_origin, target_origin));
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_content_window_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let connected = with_context_host(context, |host| node_is_connected(&host.document, node_id))
        .unwrap_or(false);
    if !connected {
        return Ok(JsValue::null());
    }
    let Ok((frame_id, state_rc)) = ensure_iframe_state(node_id, context) else {
        return Ok(JsValue::null());
    };
    let same_origin = {
        let st = state_rc.borrow();
        st.same_origin && !host_opaque_origin(&st.runtime)
    };
    if same_origin {
        bridge_decode_to_parent(
            BridgeResult::Ref {
                ref_id: 1,
                is_fn: false,
            },
            frame_id,
            None,
            context,
        )
    } else {
        let restricted = build_restricted_frame_window(context, Some(frame_id), false)?;
        Ok(JsValue::from(restricted))
    }
}

pub(crate) fn node_srcdoc_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let val = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .and_then(|el| el.get_attribute("srcdoc").map(str::to_owned))
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(val)))
}

pub(crate) fn node_srcdoc_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    string_attribute_set(this, "srcdoc", args, context)
}

pub(crate) fn node_sandbox_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let val = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .and_then(|el| el.get_attribute("sandbox").map(str::to_owned))
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(val)))
}

pub(crate) fn node_sandbox_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    string_attribute_set(this, "sandbox", args, context)
}

pub(crate) fn window_origin_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    let origin = with_context_host(_context, |host| {
        crate::host::origin_for_url(&host.url, host.opaque_origin)
    })
    .unwrap_or_else(|| "null".to_string());
    Ok(JsValue::from(js_string!(origin)))
}

pub(crate) fn window_parent_get(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let is_child = with_context_host(context, |host| host.is_child_frame).unwrap_or(false);
    if !is_child {
        return Ok(JsValue::from(context.global_object()));
    }
    let proxy = build_restricted_frame_window(context, None, true)?;
    Ok(JsValue::from(proxy))
}

pub(crate) fn window_frame_element_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::null())
}

pub(crate) fn window_open(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    // When `allow-popups` is absent (or in headless single-tab mode), `window.open` returns `null`.
    Ok(JsValue::null())
}

pub(crate) fn window_post_message(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let payload = stringify_for_message(args.first().unwrap_or(&JsValue::undefined()), context)?;
    let target_origin = post_target_origin_arg(args, 1, context)?;
    with_context_host(context, |host| {
        let origin = crate::host::origin_for_url(&host.url, host.opaque_origin);
        if target_origin_matches(&target_origin, &origin, &origin)
            && host.self_messages.len() < MAX_FRAME_MESSAGES
        {
            host.self_messages.push_back((payload, origin));
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn replaceable_setter(name: &'static str) -> NativeFn {
    match name {
        "origin" => |this, args, context| {
            let val = args.first().cloned().unwrap_or_else(JsValue::undefined);
            if let Some(obj) = this.as_object() {
                let desc = PropertyDescriptor::builder()
                    .value(val)
                    .writable(true)
                    .enumerable(true)
                    .configurable(true)
                    .build();
                let _ = obj.define_property_or_throw(js_string!("origin"), desc, context);
            }
            Ok(JsValue::undefined())
        },
        "parent" => |this, args, context| {
            let val = args.first().cloned().unwrap_or_else(JsValue::undefined);
            if let Some(obj) = this.as_object() {
                let desc = PropertyDescriptor::builder()
                    .value(val)
                    .writable(true)
                    .enumerable(true)
                    .configurable(true)
                    .build();
                let _ = obj.define_property_or_throw(js_string!("parent"), desc, context);
            }
            Ok(JsValue::undefined())
        },
        _ => |_this, _args, _context| Ok(JsValue::undefined()),
    }
}

pub(crate) fn install_frame_globals(context: &mut Context) -> Result<(), JsError> {
    let global = context.global_object();
    let doc = global
        .get(js_string!("document"), context)
        .unwrap_or_else(|_| JsValue::undefined());
    let refs = JsArray::from_iter([doc, JsValue::from(global.clone())], context);
    global
        .set(
            js_string!("__megi_bridge_refs"),
            JsValue::from(refs),
            false,
            context,
        )
        .map_err(|e| JsError::Internal(e.to_string()))?;
    for (name, len, f) in [
        ("__megi_fb_get", 3usize, frame_bridge_get as NativeFn),
        ("__megi_fb_set", 4, frame_bridge_set),
        ("__megi_fb_has", 3, frame_bridge_has),
        ("__megi_fb_call", 4, frame_bridge_call),
        ("__megi_fb_post_message", 3, frame_bridge_post_message),
        ("open", 3, window_open),
        ("postMessage", 2, window_post_message),
    ] {
        let func = FunctionObjectBuilder::new(context.realm(), native(f))
            .name(js_string!(name))
            .length(len)
            .build();
        global
            .set(js_string!(name), JsValue::from(func), false, context)
            .map_err(|e| JsError::Internal(e.to_string()))?;
    }
    define_accessor(
        context,
        &global,
        "origin",
        window_origin_get,
        Some(replaceable_setter("origin")),
    )?;
    define_accessor(
        context,
        &global,
        "parent",
        window_parent_get,
        Some(replaceable_setter("parent")),
    )?;
    define_accessor(context, &global, "top", window_parent_get, None)?;
    define_accessor(
        context,
        &global,
        "frameElement",
        window_frame_element_get,
        None,
    )?;
    context
        .eval(Source::from_bytes(
            br#"(function () {
  var caches = {};
  window.__megiWrapFrameObj = function (frameId, refId, isFn, ownerRefId) {
    if (!caches[frameId]) { caches[frameId] = {}; }
    if (!isFn && caches[frameId][refId]) { return caches[frameId][refId]; }
    var target = isFn ? function () {} : {};
    var p = new Proxy(target, {
      get: function (_t, prop) {
        if (prop === '__megi_frame_id') { return frameId; }
        if (prop === '__megi_ref_id') { return refId; }
        if (typeof prop === 'symbol') { return undefined; }
        if (prop === 'postMessage' && refId === 1) {
          return function (msg, targetOrigin) {
            return __megi_fb_post_message(frameId, msg, targetOrigin);
          };
        }
        return __megi_fb_get(frameId, refId, String(prop));
      },
      set: function (_t, prop, value) {
        if (typeof prop === 'symbol') { return false; }
        return __megi_fb_set(frameId, refId, String(prop), value);
      },
      has: function (_t, prop) {
        if (typeof prop === 'symbol') { return false; }
        return __megi_fb_has(frameId, refId, String(prop));
      },
      apply: function (_t, thisArg, argsList) {
        var recv = ownerRefId;
        if (thisArg && thisArg.__megi_frame_id === frameId) {
          recv = thisArg.__megi_ref_id;
        }
        return __megi_fb_call(frameId, refId, recv, Array.prototype.slice.call(argsList));
      }
    });
    if (!isFn) { caches[frameId][refId] = p; }
    return p;
  };
})();"#,
        ))
        .map_err(|e| JsError::Internal(e.to_string()))?;
    Ok(())
}

impl JsRuntime {
    /// Marks this runtime as having an opaque origin (`sandbox` without `allow-same-origin`, #1544).
    pub fn set_opaque_origin(&mut self, opaque: bool) {
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .opaque_origin = opaque;
    }

    /// Configures `<iframe sandbox>` capability flags (`allow_forms`, `allow_popups`, `allow_same_origin`, #1544).
    pub fn set_sandbox_flags(&mut self, flags: SandboxFlags) {
        let mut host = host_cell(&mut self.context).expect("JavaScript host installed in context");
        host.opaque_origin = !flags.allow_same_origin;
        host.allow_forms = flags.allow_forms;
        host.allow_popups = flags.allow_popups;
    }

    /// Installs a host loader for `<iframe src="...">` child documents (#1544).
    pub fn set_frame_loader(&mut self, loader: crate::host::FrameLoader) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.frame_loader = Some(loader.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .frame_loader = Some(loader);
    }

    /// Sets the current nesting depth of this document's browsing context (#1544).
    pub fn set_frame_depth(&mut self, depth: usize) {
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .frame_depth = depth;
    }

    /// Ensures every connected `<iframe>` in the document has initialized its nested browsing
    /// context and executed its inline scripts (if allowed by `sandbox`), bounded by caps.
    pub fn init_connected_frames(&mut self) {
        let iframe_nodes: Vec<NodeId> = {
            let host = host_ref(&self.context).expect("JavaScript host installed in context");
            host.document
                .get_elements_by_tag_name("iframe")
                .into_iter()
                .filter(|&id| node_is_connected(&host.document, id))
                .collect()
        };
        if iframe_nodes.is_empty() {
            return;
        }
        for node_id in iframe_nodes {
            let _ = ensure_iframe_state(node_id, &mut self.context);
        }
    }

    /// Delivers queued `window.postMessage` events across the parent/child frame tree (#1544).
    pub fn run_frame_tasks(&mut self, max_steps: usize) -> Result<usize, JsError> {
        let mut delivered = 0usize;
        for _ in 0..max_steps {
            let mut progressed = false;

            // 1) Self-targeted `window.postMessage(...)` on the parent window.
            let self_msg = host_cell(&mut self.context)
                .expect("JavaScript host installed in context")
                .self_messages
                .pop_front();
            if let Some((json, origin)) = self_msg {
                progressed = true;
                delivered += 1;
                if let Ok(data) = self.parse_json_value(&json) {
                    let window_val = JsValue::from(self.context.global_object());
                    let _ = crate::events::dispatch_message_event(
                        &window_val,
                        data,
                        &origin,
                        window_val.clone(),
                        &mut self.context,
                    );
                }
            }

            // 2) Child frame queues (self, parent -> child, and child -> parent).
            let count = host_ref(&self.context)
                .expect("JavaScript host installed in context")
                .frames
                .len();
            for idx in 0..count {
                let state_rc = host_ref(&self.context)
                    .expect("JavaScript host installed in context")
                    .frames
                    .get(idx)
                    .and_then(|slot| slot.as_ref().map(Rc::clone));
                let Some(state_rc) = state_rc else {
                    continue;
                };

                // Deliver self-targeted `window.postMessage(...)` inside child frame.
                loop {
                    let next_self = {
                        let mut st = state_rc.borrow_mut();
                        let mut child_host = host_cell(&mut st.runtime.context)
                            .expect("JavaScript host installed in context");
                        child_host.self_messages.pop_front()
                    };
                    let Some((json, origin)) = next_self else {
                        break;
                    };
                    progressed = true;
                    delivered += 1;
                    let mut state = state_rc.borrow_mut();
                    let Ok(data) = state.runtime.parse_json_value(&json) else {
                        continue;
                    };
                    let child_window = JsValue::from(state.runtime.context.global_object());
                    let _ = crate::events::dispatch_message_event(
                        &child_window,
                        data,
                        &origin,
                        child_window.clone(),
                        &mut state.runtime.context,
                    );
                    let _ = state.runtime.run_microtasks();
                }

                // Deliver parent -> child messages.
                loop {
                    let next_msg = state_rc.borrow_mut().to_child.pop_front();
                    let Some((json, origin)) = next_msg else {
                        break;
                    };
                    progressed = true;
                    delivered += 1;
                    let mut state = state_rc.borrow_mut();
                    let Ok(data) = state.runtime.parse_json_value(&json) else {
                        continue;
                    };
                    let child_window = JsValue::from(state.runtime.context.global_object());
                    let parent_proxy =
                        build_restricted_frame_window(&mut state.runtime.context, None, true)
                            .map(JsValue::from)
                            .unwrap_or_else(|_| JsValue::null());
                    let _ = crate::events::dispatch_message_event(
                        &child_window,
                        data,
                        &origin,
                        parent_proxy,
                        &mut state.runtime.context,
                    );
                    let _ = state.runtime.run_microtasks();
                }

                // Drain child -> parent messages.
                loop {
                    let next_reply = {
                        let mut st = state_rc.borrow_mut();
                        host_cell(&mut st.runtime.context)
                            .ok()
                            .and_then(|mut child_host| {
                                child_host.parent_outbox.as_mut()?.pop_front()
                            })
                    };
                    let Some((json, child_origin, target_origin)) = next_reply else {
                        break;
                    };
                    let (parent_origin, sender_origin) = {
                        let host =
                            host_ref(&self.context).expect("JavaScript host installed in context");
                        (
                            crate::host::origin_for_url(&host.url, host.opaque_origin),
                            child_origin.clone(),
                        )
                    };
                    if !target_origin_matches(&target_origin, &parent_origin, &sender_origin) {
                        continue;
                    }
                    progressed = true;
                    delivered += 1;
                    let Ok(data) = self.parse_json_value(&json) else {
                        continue;
                    };
                    let same_origin_child = {
                        let st = state_rc.borrow();
                        st.same_origin && !host_opaque_origin(&st.runtime)
                    };
                    let source = if same_origin_child {
                        bridge_decode_to_parent(
                            BridgeResult::Ref {
                                ref_id: 1,
                                is_fn: false,
                            },
                            idx as u32,
                            None,
                            &mut self.context,
                        )
                        .unwrap_or_else(|_| JsValue::null())
                    } else {
                        build_restricted_frame_window(&mut self.context, Some(idx as u32), false)
                            .map(JsValue::from)
                            .unwrap_or_else(|_| JsValue::null())
                    };
                    let parent_window = JsValue::from(self.context.global_object());
                    let _ = crate::events::dispatch_message_event(
                        &parent_window,
                        data,
                        &child_origin,
                        source,
                        &mut self.context,
                    );
                }
            }

            if !progressed {
                break;
            }
            let _ = self.run_microtasks();
        }
        Ok(delivered)
    }

    /// Snapshots initialized/mutated child `<iframe>` documents keyed by host element `NodeId` (#1544).
    pub fn snapshot_frame_documents(&self) -> Vec<(NodeId, Document, SandboxFlags, String)> {
        let host = host_ref(&self.context).expect("JavaScript host installed in context");
        let mut out = Vec::new();
        for state_rc in host.frames.iter().flatten() {
            let state = state_rc.borrow();
            let child_doc = host_ref(&state.runtime.context)
                .expect("JavaScript host installed in context")
                .document
                .clone();
            out.push((state.node_id, child_doc, state.sandbox, state.url.clone()));
        }
        out
    }
}
