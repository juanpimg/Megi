use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropPhase {
    Capture,
    Target,
    Bubble,
}

pub(crate) fn phase_number(phase: PropPhase) -> f64 {
    match phase {
        PropPhase::Capture => 1.0,
        PropPhase::Target => 2.0,
        PropPhase::Bubble => 3.0,
    }
}

pub(crate) fn is_budget_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("loop iteration limit")
        || lower.contains("runtime limit")
        || lower.contains("recursion limit")
        || lower.contains("stack size")
}

pub(crate) fn propagation_path(
    context: &mut Context,
    target: TargetKey,
    bubbles: bool,
) -> JsResult<Vec<(PropPhase, TargetKey)>> {
    // Dispatch on `window`: the window is the target (`AT_TARGET`, phase 2), with capture
    // and bubble listeners both running there; there are no ancestors above it.
    if target == TargetKey::Window {
        return Ok(vec![(PropPhase::Target, TargetKey::Window)]);
    }
    // Dispatch on `document`: window is the only capture ancestor; document listeners run
    // at the target (phase 2), then a single bubble leg to window when requested.
    if target == TargetKey::Document {
        let mut path = vec![
            (PropPhase::Capture, TargetKey::Window),
            (PropPhase::Target, TargetKey::Document),
        ];
        if bubbles {
            path.push((PropPhase::Bubble, TargetKey::Window));
        }
        return Ok(path);
    }
    let mut ancestors: Vec<TargetKey> = Vec::new();
    let mut connected = !matches!(target, TargetKey::Node(_));
    if let TargetKey::Node(handle) = target
        && let Ok(node) = node_id_of(context, handle)
    {
        let (chain, root) = {
            let host = host_ref(context)?;
            let root = host.document.root();
            let mut ids = Vec::new();
            let mut current = Some(node);
            while let Some(id) = current {
                ids.push(id);
                current = host.document.try_node(id).and_then(|node| node.parent);
            }
            (ids, root)
        };
        // #1743: DOM §2.9 — a detached node's event path stops at its own root; only nodes
        // connected to the document reach `Document`/`Window`.
        connected = chain.last().copied() == Some(root);
        for id in chain.iter().skip(1) {
            if *id == root {
                continue; // represented by TargetKey::Document, not a node wrapper
            }
            if let Ok(handle) = host_cell(context)?.handle_for(*id) {
                ancestors.push(TargetKey::Node(handle));
            }
        }
    }
    let mut path = Vec::new();
    // Capture ancestors above the target, top-down. The target itself appears exactly once:
    // at-target listeners run in the AT_TARGET phase regardless of their capture flag.
    if connected && !matches!(target, TargetKey::Window) {
        path.push((PropPhase::Capture, TargetKey::Window));
        if !matches!(target, TargetKey::Document) {
            path.push((PropPhase::Capture, TargetKey::Document));
        }
    }
    for ancestor in ancestors.iter().rev() {
        path.push((PropPhase::Capture, *ancestor));
    }
    path.push((PropPhase::Target, target));
    if bubbles {
        for ancestor in &ancestors {
            path.push((PropPhase::Bubble, *ancestor));
        }
        if connected && !matches!(target, TargetKey::Document) {
            path.push((PropPhase::Bubble, TargetKey::Document));
        }
        if connected && !matches!(target, TargetKey::Window) {
            path.push((PropPhase::Bubble, TargetKey::Window));
        }
    }
    Ok(path)
}

/// JS value used as `event.currentTarget`/`this` while visiting `target`.
pub(crate) fn target_js_value(context: &mut Context, target: TargetKey) -> JsResult<JsValue> {
    match target {
        TargetKey::Window => Ok(JsValue::from(context.global_object())),
        TargetKey::Document => global_get(context, "document"),
        TargetKey::Node(handle) => crate::runtime::wrap_handle_public(context, handle)
            .map_err(|error| js_error(error.to_string())),
    }
}

/// Invokes one listener, isolating ordinary exceptions (budget errors still propagate).
pub(crate) fn call_listener(
    context: &mut Context,
    callback: &JsObject,
    current_target: &JsValue,
    event: &JsObject,
    passive: bool,
    once: bool,
    slot: u32,
) -> JsResult<()> {
    if let Some(dispatch) = &mut host_cell(context)?.dispatch {
        dispatch.passive = passive;
    }
    // #1741: DOM §2.7 — a `once` listener is removed *before* invoking its callback, so a
    // slot reused by a listener added inside the callback cannot be removed with it.
    if once {
        let removed = {
            let mut host = host_cell(context)?;
            let before = host.listeners.len();
            host.listeners.retain(|listener| listener.slot != slot);
            if host.listeners.len() < before {
                host.free_listener_slots.push(slot);
                true
            } else {
                false
            }
        };
        if removed {
            let _ = array_put(
                context,
                "__megi_listeners",
                u64::from(slot),
                JsValue::undefined(),
            );
        }
    }
    // No host borrow may survive this call: the callback can re-enter any binding.
    let result = callback.call(current_target, &[JsValue::from(event.clone())], context);
    if let Some(dispatch) = &mut host_cell(context)?.dispatch {
        dispatch.passive = false;
    }
    match result {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            if is_budget_message(&message) {
                return Err(js_error(message));
            }
            {
                let mut host = host_cell(context)?;
                host.last_event_error = Some(message.clone());
                host.push_console(crate::host::ConsoleEntry {
                    level: "error".to_string(),
                    message,
                });
            }
            Ok(())
        }
    }
}

pub(crate) fn deliver(
    context: &mut Context,
    target: TargetKey,
    phase: PropPhase,
    event_type: &str,
    event: &JsObject,
) -> JsResult<()> {
    let current_target = target_js_value(context, target)?;
    let _ = event.set(
        js_string!("currentTarget"),
        current_target.clone(),
        false,
        context,
    );
    let _ = event.set(
        js_string!("eventPhase"),
        JsValue::from(phase_number(phase)),
        false,
        context,
    );
    let at_target = phase == PropPhase::Target;
    // Snapshot (slot, once, passive, capture) while the borrow is alive. The
    // capture flag is copied into the tuple so the AT_TARGET sort never needs to
    // re-scan `host.listeners` from inside the sort closure.
    let listeners: Vec<(u32, bool, bool, bool)> = {
        let host = host_ref(context)?;
        let mut matching: Vec<(u32, bool, bool, bool)> = host
            .listeners
            .iter()
            .filter(|listener| listener.target == target && listener.event_type == event_type)
            .filter(|listener| at_target || listener.capture == (phase == PropPhase::Capture))
            .map(|listener| {
                (
                    listener.slot,
                    listener.once,
                    listener.passive,
                    listener.capture,
                )
            })
            .collect();
        if at_target {
            // AT_TARGET: capture listeners first, then bubble listeners (same phase).
            matching.sort_by_key(|(_, _, _, capture)| !capture);
        }
        matching
    };
    for (slot, once, passive, _) in listeners {
        let immediate = host_ref(context)?
            .dispatch
            .map(|dispatch| dispatch.stop_immediate)
            .unwrap_or(false);
        if immediate {
            return Ok(());
        }
        // A listener removed by an earlier callback in this dispatch must not run.
        let registered = host_ref(context)?
            .listeners
            .iter()
            .any(|listener| listener.slot == slot);
        if !registered {
            continue;
        }
        let callback = array_get(context, "__megi_listeners", u64::from(slot))?;
        if let Some(callback) = callback.as_object() {
            call_listener(
                context,
                &callback,
                &current_target,
                event,
                passive,
                once,
                slot,
            )?;
        }
    }
    // Inline `on<event>` handlers run at the target (bubble phase), after regular listeners.
    if phase == PropPhase::Target {
        let inline_slot = {
            let host = host_ref(context)?;
            host.inline_handlers
                .iter()
                .find(|handler| handler.target == target && handler.event_type == event_type)
                .map(|handler| handler.slot)
        };
        if let Some(slot) = inline_slot {
            let callback = array_get(context, "__megi_inline_handlers", u64::from(slot))?;
            if let Some(callback) = callback.as_object() {
                call_listener(
                    context,
                    &callback,
                    &current_target,
                    event,
                    false,
                    false,
                    slot,
                )?;
            }
        }
    }
    Ok(())
}

pub(crate) fn set_event_prototype(context: &mut Context, object: &JsObject, ctor_name: &str) {
    if let Ok(ctor) = context.global_object().get(js_string!(ctor_name), context)
        && let Some(ctor_obj) = ctor.as_object()
        && let Ok(proto) = ctor_obj.get(js_string!("prototype"), context)
        && let Some(proto_obj) = proto.as_object()
    {
        let _ = object.set_prototype(Some(proto_obj));
    }
}

pub(crate) fn build_ui_event(
    context: &mut Context,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    kind: &JsUiEventKind,
) -> JsValue {
    let event = build_event(
        context,
        event_type,
        bubbles,
        cancelable,
        JsValue::undefined(),
    );
    if let Some(object) = event.as_object() {
        match kind {
            JsUiEventKind::Simple => {}
            JsUiEventKind::Keyboard {
                key,
                code,
                ctrl_key,
                shift_key,
                alt_key,
                meta_key,
                repeat,
            } => {
                set_event_prototype(context, &object, "KeyboardEvent");
                let _ = object.set(js_string!("key"), js_string!(key.clone()), false, context);
                let _ = object.set(js_string!("code"), js_string!(code.clone()), false, context);
                let _ = object.set(js_string!("ctrlKey"), *ctrl_key, false, context);
                let _ = object.set(js_string!("shiftKey"), *shift_key, false, context);
                let _ = object.set(js_string!("altKey"), *alt_key, false, context);
                let _ = object.set(js_string!("metaKey"), *meta_key, false, context);
                let _ = object.set(js_string!("repeat"), *repeat, false, context);
            }
            JsUiEventKind::Input { data, input_type } => {
                set_event_prototype(context, &object, "InputEvent");
                let data_value = match data {
                    Some(text) => JsValue::from(js_string!(text.clone())),
                    None => JsValue::null(),
                };
                let _ = object.set(js_string!("data"), data_value, false, context);
                let _ = object.set(
                    js_string!("inputType"),
                    js_string!(input_type.clone()),
                    false,
                    context,
                );
            }
            JsUiEventKind::Focus => {
                set_event_prototype(context, &object, "FocusEvent");
                let _ = object.set(js_string!("relatedTarget"), JsValue::null(), false, context);
            }
            JsUiEventKind::Composition { data } => {
                set_event_prototype(context, &object, "CompositionEvent");
                let _ = object.set(js_string!("data"), js_string!(data.clone()), false, context);
            }
            JsUiEventKind::Animation {
                animation_name,
                elapsed_time,
                pseudo_element,
            } => {
                set_event_prototype(context, &object, "AnimationEvent");
                let _ = object.set(
                    js_string!("animationName"),
                    js_string!(animation_name.clone()),
                    false,
                    context,
                );
                let _ = object.set(js_string!("elapsedTime"), *elapsed_time, false, context);
                let _ = object.set(
                    js_string!("pseudoElement"),
                    js_string!(pseudo_element.clone()),
                    false,
                    context,
                );
            }
            JsUiEventKind::Transition {
                property_name,
                elapsed_time,
                pseudo_element,
            } => {
                set_event_prototype(context, &object, "TransitionEvent");
                let _ = object.set(
                    js_string!("propertyName"),
                    js_string!(property_name.clone()),
                    false,
                    context,
                );
                let _ = object.set(js_string!("elapsedTime"), *elapsed_time, false, context);
                let _ = object.set(
                    js_string!("pseudoElement"),
                    js_string!(pseudo_element.clone()),
                    false,
                    context,
                );
            }
        }
    }
    event
}

pub(crate) fn dispatch_built_event(
    context: &mut Context,
    target: JsEventTarget,
    event_type: &str,
    bubbles: bool,
    event: JsValue,
) -> Result<bool, CrateJsError> {
    let target = match target {
        JsEventTarget::Window => TargetKey::Window,
        JsEventTarget::Document => TargetKey::Document,
        JsEventTarget::Node(node) => {
            let handle = host_cell(context)
                .map_err(|error| CrateJsError::Internal(error.to_string()))?
                .handle_for(node)
                .map_err(CrateJsError::Dom)?;
            TargetKey::Node(handle)
        }
    };
    let target_js = match target {
        TargetKey::Window => JsValue::from(context.global_object()),
        TargetKey::Document => global_get(context, "document")
            .map_err(|error| CrateJsError::Internal(error.to_string()))?,
        TargetKey::Node(handle) => crate::runtime::wrap_handle_public(context, handle)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?,
    };
    let Some(event_object) = event.as_object() else {
        return Err(CrateJsError::Internal(
            "event object is not an object".to_string(),
        ));
    };
    let _ = event_object.set(js_string!("target"), target_js.clone(), false, context);
    let path = propagation_path(context, target, bubbles)
        .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
    // Stack the dispatch state so a nested dispatch cannot clobber the outer one (#648).
    let previous_dispatch = host_cell(context)
        .map_err(|error| CrateJsError::Internal(error.to_string()))?
        .dispatch
        .take();
    let _dispatch_guard = DispatchStateGuard::new(context, previous_dispatch)
        .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
    host_cell(context)
        .map_err(|error| CrateJsError::Internal(error.to_string()))?
        .dispatch = Some(DispatchState::default());
    for (phase, key) in &path {
        let stopped = host_ref(context)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?
            .dispatch
            .map(|dispatch| dispatch.stop_propagation)
            .unwrap_or(false);
        if stopped {
            break;
        }
        deliver(context, *key, *phase, event_type, &event_object)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?;
        let immediate = host_ref(context)
            .map_err(|error| CrateJsError::Runtime(error.to_string()))?
            .dispatch
            .map(|dispatch| dispatch.stop_immediate)
            .unwrap_or(false);
        if immediate {
            break;
        }
    }
    let _ = event_object.set(js_string!("currentTarget"), JsValue::null(), false, context);
    let _ = event_object.set(js_string!("eventPhase"), JsValue::from(0.0), false, context);
    let prevented = host_ref(context)
        .map_err(|error| CrateJsError::Runtime(error.to_string()))?
        .dispatch
        .map(|dispatch| dispatch.prevent_default)
        .unwrap_or(false);
    let _ = event_object.set(
        js_string!("defaultPrevented"),
        JsValue::from(prevented),
        false,
        context,
    );
    Ok(!prevented)
}

/// Dispatches a synthetic event from Rust to the current realm's listeners.
///
/// `target` selects `window`, the `document` object or a DOM node (bubbling through its
/// ancestors). Returns `Ok(false)` when a listener called `preventDefault()`.
pub(crate) fn dispatch_rust_event(
    context: &mut Context,
    target: JsEventTarget,
    event_type: &str,
    bubbles: bool,
) -> Result<bool, CrateJsError> {
    let event = build_event(context, event_type, bubbles, true, JsValue::undefined());
    dispatch_built_event(context, target, event_type, bubbles, event)
}

/// Dispatches a structured UI event (`KeyboardEvent`, `InputEvent`, `FocusEvent`, or
/// `Event`) from Rust into the realm's capture/target/bubble flow (#1504).
pub(crate) fn dispatch_rust_ui_event(
    context: &mut Context,
    target: JsEventTarget,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    kind: &JsUiEventKind,
) -> Result<bool, CrateJsError> {
    let event = build_ui_event(context, event_type, bubbles, cancelable, kind);
    dispatch_built_event(context, target, event_type, bubbles, event)
}

// ---------------------------------------------------------------------------
// matchMedia / viewport / scroll
// ---------------------------------------------------------------------------
