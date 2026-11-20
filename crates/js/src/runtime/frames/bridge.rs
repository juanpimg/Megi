use super::*;

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
    // ARCH-EXCEPTION(#1853): Boa native functions need an owned handle to the child realm.
    // Plan: #1853 — return a NodeId and resolve via host on each access.
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
