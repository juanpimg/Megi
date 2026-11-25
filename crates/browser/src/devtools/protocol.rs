//! DevTools Protocol JSON-RPC commands served on the WebSocket target.
//!
//! DOM commands read the real document arena attached by the shell: `querySelector` is
//! WHATWG DOM §4.2.3, `outerHTML` is WHATWG HTML §13.3 and `getComputedStyle` is CSSOM §6.
//! With no attached page only `Runtime.evaluate` is available, backed by the original
//! isolated realm (the headless/no-document fallback).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use megi_webview::core::{Document, NodeId};
use megi_webview::css::ComputedStyle;
use megi_webview::net::NetworkEvent;
use megi_webview::render::{
    RenderOutput, RenderTimings, computed_style_pairs, evaluate_expression, evaluate_on_document,
};
use serde_json::{Value, json};

/// Snapshot of the currently loaded page used by DevTools commands.
///
/// Node ids are the arena's real packed handles (`NodeId::get`/`from_raw`), so ids returned
/// by `DOM.getDocument`/`DOM.querySelector` resolve against this document without a side table.
#[derive(Clone, Debug)]
pub struct DevToolsPage {
    pub(super) document: Document,
    pub(super) styles: HashMap<NodeId, ComputedStyle>,
    pub(super) url: String,
    pub(super) network: Vec<NetworkEvent>,
    pub(super) timings: RenderTimings,
}

impl DevToolsPage {
    /// Creates a page snapshot from an owned arena, its style snapshot and the real
    /// loader observations and phase timings collected while rendering it.
    pub fn new(
        document: Document,
        styles: HashMap<NodeId, ComputedStyle>,
        url: String,
        network: Vec<NetworkEvent>,
        timings: RenderTimings,
    ) -> Self {
        Self {
            document,
            styles,
            url,
            network,
            timings,
        }
    }

    /// Attaches a real render result (DOM + cascade snapshot + URL + request log + timings).
    pub fn from_render(output: &RenderOutput) -> Self {
        Self::new(
            output.document.clone(),
            output.styles.clone(),
            output.document_url.as_str().to_owned(),
            output.stats.network.clone(),
            output.stats.timings,
        )
    }
}

/// Page state shared between the accept loop and the per-connection RPC threads.
pub(super) type SharedPage = Arc<Mutex<Option<DevToolsPage>>>;

/// Dispatches one JSON-RPC request into the WebSocket frames to send back.
///
/// `Network.enable` emits CDP/Firefox events first (one per real loader observation) and
/// then its result; other commands keep the single-response shape.
pub(super) fn rpc_messages(payload: &[u8], page: &SharedPage) -> Vec<String> {
    let request: Value = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(_) => return vec![rpc_error(Value::Null, -32700, "Parse error")],
    };
    if request.get("method").and_then(Value::as_str) == Some("Network.enable") {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        return match lock_page(page).as_ref() {
            Some(attached) => super::network::enable(&attached.network, id),
            None => vec![rpc_error(
                id,
                -32000,
                "no page attached to the DevTools target",
            )],
        };
    }
    vec![rpc_response(payload, page)]
}

/// Dispatches one JSON-RPC request; every command is answered with one JSON-RPC response.
pub(super) fn rpc_response(payload: &[u8], page: &SharedPage) -> String {
    let request: Value = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(_) => return rpc_error(Value::Null, -32700, "Parse error"),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    match request.get("method").and_then(Value::as_str) {
        Some("Runtime.evaluate") => {
            let expression = request
                .get("params")
                .and_then(|params| params.get("expression"))
                .and_then(Value::as_str)
                .unwrap_or("");
            // Hold the lock across evaluation so DOM mutations from scripts are written
            // back to the same snapshot served by the DOM/CSS commands.
            let mut guard = lock_page(page);
            let text = match guard.as_mut() {
                Some(attached) => evaluate_on_document(
                    &mut attached.document,
                    &attached.styles,
                    expression,
                    &attached.url,
                ),
                None => evaluate_expression(expression),
            };
            match text {
                Ok(text) => {
                    // Completion values are text; parse primitives into JSON types so a
                    // client sees the number 2 for `1+1`, not the string "2".
                    let value =
                        serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text.clone()));
                    let kind = match &value {
                        Value::Number(_) => "number",
                        Value::Bool(_) => "boolean",
                        Value::Null => "object",
                        _ => "string",
                    };
                    json!({
                        "id": id,
                        "result": {
                            "result": { "type": kind, "value": value, "description": text },
                        },
                    })
                    .to_string()
                }
                Err(message) => rpc_error(id, -32000, message),
            }
        }
        Some(
            method @ ("DOM.getDocument"
            | "DOM.querySelector"
            | "DOM.getOuterHTML"
            | "CSS.getComputedStyle"),
        ) => {
            let guard = lock_page(page);
            match guard.as_ref() {
                Some(attached) => dom_response(attached, id, method, request.get("params")),
                None => rpc_error(id, -32000, "no page attached to the DevTools target"),
            }
        }
        Some(method @ ("Performance.getMetrics" | "Performance.getTrace")) => {
            let guard = lock_page(page);
            match guard.as_ref() {
                Some(attached) => match method {
                    "Performance.getMetrics" => {
                        super::performance::get_metrics(&attached.timings, id)
                    }
                    _ => super::performance::get_trace(&attached.timings, id),
                },
                None => rpc_error(id, -32000, "no page attached to the DevTools target"),
            }
        }
        Some(method) => rpc_error(id, -32601, format!("Method not found: {method}")),
        None => rpc_error(id, -32600, "Invalid Request"),
    }
}

pub(super) fn lock_page(page: &SharedPage) -> std::sync::MutexGuard<'_, Option<DevToolsPage>> {
    page.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// DOM/CSS domain dispatch against the attached document (WHATWG DOM §4.2.3 / CSSOM §6).
fn dom_response(page: &DevToolsPage, id: Value, method: &str, params: Option<&Value>) -> String {
    match method {
        "DOM.getDocument" => {
            let root = page.document.root();
            json!({
                "id": id,
                "result": { "root": { "nodeId": root.get(), "nodeName": "#document" } },
            })
            .to_string()
        }
        "DOM.querySelector" => {
            let node = params
                .and_then(|params| params.get("nodeId"))
                .and_then(Value::as_u64)
                .and_then(|raw| u32::try_from(raw).ok())
                .and_then(NodeId::from_raw)
                .unwrap_or_else(|| page.document.root());
            let selector = params
                .and_then(|params| params.get("selector"))
                .and_then(Value::as_str)
                .unwrap_or("");
            // WHATWG DOM §4.2.3: match the first descendant in tree order within `node`.
            let found =
                megi_webview::css::query::query_selector_from(&page.document, node, selector);
            json!({ "id": id, "result": { "nodeId": found.map(NodeId::get).unwrap_or(0) } })
                .to_string()
        }
        "DOM.getOuterHTML" => match live_node(page, params) {
            Some(node) => {
                // WHATWG HTML §13.3: serialize this node itself, not its children only.
                let html = megi_webview::html::serialize_outer_html(&page.document, node);
                json!({ "id": id, "result": { "outerHTML": html } }).to_string()
            }
            None => rpc_error(
                id,
                -32000,
                "nodeId is not a live node of the attached document",
            ),
        },
        "CSS.getComputedStyle" => match live_node(page, params) {
            Some(node) => {
                // CSSOM §6: computed declarations from the live cascade, not inline style.
                let properties: Vec<Value> = page
                    .styles
                    .get(&node)
                    .map(computed_style_pairs)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, value)| json!({ "name": name, "value": value }))
                    .collect();
                json!({ "id": id, "result": { "computedStyle": properties } }).to_string()
            }
            None => rpc_error(
                id,
                -32000,
                "nodeId is not a live node of the attached document",
            ),
        },
        _ => rpc_error(id, -32601, "Method not found"),
    }
}

/// Resolves a JSON-RPC `nodeId` to a node that still exists in the attached arena.
fn live_node(page: &DevToolsPage, params: Option<&Value>) -> Option<NodeId> {
    let raw = params?.get("nodeId")?.as_u64()?;
    let node = NodeId::from_raw(u32::try_from(raw).ok()?)?;
    page.document.try_node(node)?;
    Some(node)
}

pub(super) fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> String {
    json!({ "id": id, "error": { "code": code, "message": message.into() } }).to_string()
}
