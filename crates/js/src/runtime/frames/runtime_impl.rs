use super::*;

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
