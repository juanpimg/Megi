//! Worker DOM proxy call dispatch (F6-WORKERS-DOM).

use super::*;

impl JsRuntime {
    /// Handles a `__megi_dom_call` message. Returns `Some(result)` when the message was a
    /// proxy call (so it must not reach the worker's `onmessage`).
    pub(crate) fn handle_dom_proxy_call(
        &mut self,
        id: u32,
        event: &JsValue,
    ) -> Result<Option<JsValue>, JsError> {
        // F6-WORKERS-DOM: intercept proxied DOM calls, execute them against the owning
        // document, and queue a reply for the worker's next deterministic pump step.
        if let Some(object) = event.as_object()
            && let Ok(call) = crate::events::prop(&object, "__megi_dom_call", &mut self.context)
            && let Some(call) = call.as_object()
        {
            let method = crate::events::prop(&call, "method", &mut self.context)?
                .to_string(&mut self.context)
                .map(|text| text.to_std_string_escaped())
                .unwrap_or_default();
            let seq = crate::events::prop(&call, "seq", &mut self.context)?
                .as_number()
                .unwrap_or(0.0) as u64;
            let args = crate::events::prop(&call, "args", &mut self.context)?;
            let mut worker_nodes: Vec<NodeId> = if let Ok(host) = host_ref(&self.context) {
                match host.workers.get(id as usize).and_then(|slot| slot.as_ref()) {
                    Some(worker) => worker.dom_nodes.clone(),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };
            let first = args
                .as_object()
                .and_then(|object| object.get(0, &mut self.context).ok())
                .unwrap_or(JsValue::null());
            let mut arg = |index: u32| -> String {
                args.as_object()
                    .and_then(|object| object.get(index, &mut self.context).ok())
                    .and_then(|value| value.to_string(&mut self.context).ok())
                    .map(|text| text.to_std_string_escaped())
                    .unwrap_or_default()
            };
            let result = match method.as_str() {
                "getElementById" => {
                    let id = first
                        .to_string(&mut self.context)
                        .map(|text| text.to_std_string_escaped())
                        .unwrap_or_default();
                    match host_ref(&self.context)
                        .ok()
                        .and_then(|host| host.document.get_element_by_id(&id))
                    {
                        Some(node) => {
                            let token = worker_nodes.len();
                            worker_nodes.push(node);
                            JsValue::from(token as u64)
                        }
                        None => JsValue::null(),
                    }
                }
                "textContent" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let text = worker_nodes
                        .get(token)
                        .and_then(|node| {
                            host_ref(&self.context)
                                .ok()
                                .map(|host| host.document.text_content(*node))
                        })
                        .unwrap_or_default();
                    JsValue::from(js_string!(text))
                }
                "getElementsByTagName" => {
                    let tag = arg(0).to_ascii_lowercase();
                    let mut ordinals = Vec::new();
                    if let Ok(host) = host_ref(&self.context) {
                        for (ordinal, node) in host
                            .document
                            .descendants_inclusive(host.document.root())
                            .enumerate()
                        {
                            if host
                                .document
                                .try_node(node)
                                .and_then(|node| node.as_element())
                                .is_some_and(|element| element.local_name == tag)
                            {
                                ordinals.push(JsValue::from(ordinal as u64));
                            }
                        }
                    }
                    JsValue::from(JsArray::from_iter(ordinals, &mut self.context))
                }
                "getComputedStyle" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let property = arg(1);
                    let value = worker_nodes
                        .get(token)
                        .copied()
                        .and_then(|node| {
                            host_ref(&self.context).ok().and_then(|host| {
                                host.document
                                    .try_node(node)
                                    .and_then(|node| node.as_element())
                                    .and_then(|element| element.get_attribute("style"))
                                    .map(str::to_owned)
                            })
                        })
                        .and_then(|style| {
                            megi_css::parser::parse_list_of_declarations(&style)
                                .into_iter()
                                .find(|declaration| declaration.name == property)
                                .map(|declaration| declaration.raw_value)
                        })
                        .unwrap_or_default();
                    JsValue::from(js_string!(value))
                }
                "listen" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let event_type = arg(1);
                    if let Some(node) = worker_nodes.get(token).copied()
                        && let Ok(mut host) = host_cell(&mut self.context)
                        && host.worker_dom_listeners.len() < MAX_WORKER_DOM_LISTENERS
                        && !host
                            .worker_dom_listeners
                            .iter()
                            .any(|(w, n, t, _)| *w == id && *n == node && t == &event_type)
                    {
                        host.worker_dom_listeners
                            .push((id, node, event_type, token as u64));
                    }
                    return Ok(Some(JsValue::undefined()));
                }
                "classList" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let operation = arg(1);
                    let name = arg(2);
                    let mut result = JsValue::null();
                    if let Some(node) = worker_nodes.get(token).copied() {
                        let current = host_ref(&self.context)
                            .ok()
                            .and_then(|host| {
                                host.document
                                    .try_node(node)
                                    .and_then(|node| node.as_element())
                                    .and_then(|element| element.get_attribute("class"))
                                    .map(str::to_owned)
                            })
                            .unwrap_or_default();
                        let mut classes: Vec<String> = current
                            .split_ascii_whitespace()
                            .map(str::to_owned)
                            .collect();
                        let position = classes.iter().position(|class| class == &name);
                        let answer = match operation.as_str() {
                            "contains" => position.is_some(),
                            "add" => {
                                if position.is_none() {
                                    classes.push(name.clone());
                                }
                                false
                            }
                            "remove" => {
                                if let Some(index) = position {
                                    classes.remove(index);
                                }
                                false
                            }
                            "toggle" => {
                                if let Some(index) = position {
                                    classes.remove(index);
                                    false
                                } else {
                                    classes.push(name.clone());
                                    true
                                }
                            }
                            _ => false,
                        };
                        if operation != "contains"
                            && let Some(mut host) = host_cell(&mut self.context).ok()
                            && let Some(element) = host
                                .document
                                .try_node_mut(node)
                                .and_then(|node| node.as_element_mut())
                        {
                            element
                                .set_attribute(DomAttribute::new("class", classes.join(" ")), true);
                        }
                        result = JsValue::from(answer);
                    }
                    result
                }
                "setAttribute" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let name = arg(1);
                    let value = arg(2);
                    if let Some(node) = worker_nodes.get(token).copied()
                        && let Some(mut host) = host_cell(&mut self.context).ok()
                    {
                        let attribute = DomAttribute::new(name, value);
                        if let Some(element) = host
                            .document
                            .try_node_mut(node)
                            .and_then(|node| node.as_element_mut())
                        {
                            element.set_attribute(attribute, true);
                        }
                    }
                    JsValue::null()
                }
                "setTextContent" => {
                    let token = first.as_number().unwrap_or(0.0) as usize;
                    let text = arg(1);
                    if let Some(node) = worker_nodes.get(token).copied()
                        && let Some(mut host) = host_cell(&mut self.context).ok()
                    {
                        let _ = host.document.set_text_content(node, &text);
                    }
                    JsValue::null()
                }
                _ => JsValue::null(),
            };
            let result_json = crate::runtime::json_state(&result, &mut self.context);
            let reply =
                format!("{{\"__megi_dom_reply\":{{\"seq\":{seq},\"result\":{result_json}}}}}");
            let mut queue_full = false;
            if let Ok(mut host) = host_cell(&mut self.context)
                && let Some(worker) = host
                    .workers
                    .get_mut(id as usize)
                    .and_then(|slot| slot.as_mut())
            {
                worker.dom_nodes = worker_nodes;
                // #2157: share MAX_WORKER_MESSAGES backpressure on DOM proxy replies too.
                queue_full = !worker.push_to_worker(reply);
            }
            if queue_full {
                return Err(JsError::Runtime("worker message queue is full".to_string()));
            }
            return Ok(Some(JsValue::undefined()));
        }
        Ok(None)
    }
}

/// Cap on worker DOM event subscriptions (anti-DoS).
const MAX_WORKER_DOM_LISTENERS: usize = 512;
