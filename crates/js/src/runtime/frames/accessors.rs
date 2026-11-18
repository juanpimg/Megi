use super::*;

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
