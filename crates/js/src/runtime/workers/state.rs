use super::*;

// ---------------------------------------------------------------------------
// Deterministic cooperative Web Workers (round 30; no threads)
// ---------------------------------------------------------------------------

/// One live worker realm plus its FIFO queues and main-side callbacks.
pub(crate) struct WorkerState {
    pub runtime: JsRuntime,
    pub terminated: bool,
    pub to_worker: VecDeque<String>,
    /// Node handles exposed to this worker through the DOM proxy (F6-WORKERS-DOM).
    pub dom_nodes: Vec<NodeId>,
    pub onmessage: Vec<u32>,
    pub onmessage_handler: Option<u32>,
    pub onerror: Option<u32>,
    pub pending_error: Option<String>,
}

pub(crate) const MAX_WORKERS: usize = 8;
pub(crate) const MAX_WORKER_MESSAGES: usize = 256;

impl JsRuntime {
    /// Sets the maximum number of live DOM nodes a script may grow the document to (#657).
    ///
    /// Script-driven arena growth (`cloneNode(true)` in a loop, parsed HTML setters) is rejected
    /// with a `QuotaExceededError` once the document reaches the budget.
    pub fn set_max_dom_nodes(&mut self, max_nodes: usize) {
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .max_dom_nodes = max_nodes;
    }

    /// Registers a same-origin framed document for an `<iframe>` (#1544).
    pub fn set_frame_document(&mut self, iframe: NodeId, document: megi_core::Document) -> u32 {
        let mut host = host_cell(&mut self.context).expect("JavaScript host installed in context");
        let handle = host.frame_next_handle;
        host.frame_next_handle = host.frame_next_handle.wrapping_add(1);
        host.frame_documents.insert(handle, document);
        host.iframe_frames.insert(iframe, handle);
        handle
    }

    /// Whether any live WebSocket transports remain (#1534).
    pub fn has_websockets(&self) -> bool {
        !host_ref(&self.context)
            .expect("JavaScript host installed in context")
            .websockets
            .is_empty()
    }

    /// Delivers pending WebSocket transport events to JS handlers (#1534).
    /// Returns the number of instances that received events.
    pub fn poll_websockets(&mut self) -> usize {
        crate::websocket::poll(&mut self.context)
    }

    /// Installs the WebSocket transport provider (the host owns the socket).
    pub fn set_websocket_provider(&mut self, provider: crate::host::WebSocketProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.websocket = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .websocket_provider = Some(provider);
    }

    /// Installs the `fetch()` provider (the host owns all network/filesystem I/O).
    /// Without a provider `fetch()` rejects with a typed error (documented).
    pub fn set_fetch_provider(&mut self, provider: crate::host::FetchProvider) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.fetch = Some(provider.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .fetch_provider = Some(provider);
    }

    /// Installs the worker script loader (`url → source text`; the host owns all I/O).
    pub fn set_worker_loader(&mut self, loader: crate::host::WorkerLoader) {
        if let Some(slot) = crate::providers::providers_mut(&mut self.context) {
            slot.worker_loader = Some(loader.clone());
        }
        host_cell(&mut self.context)
            .expect("JavaScript host installed in context")
            .worker_loader = Some(loader);
    }

    /// Creates an isolated worker realm without DOM globals (only timers/console/
    /// `postMessage`/`onmessage`/`addEventListener('message')`).
    pub(crate) fn new_worker(id: u32) -> Result<Self, JsError> {
        let mut runtime =
            Self::with_document_realm(Document::new(), "about:blank".to_string(), true)?;
        {
            let mut host =
                host_cell(&mut runtime.context).expect("JavaScript host installed in context");
            host.worker_id = Some(id);
            host.worker_outbox = Some(VecDeque::new());
        }
        Ok(runtime)
    }

    /// Drains the deterministic worker queues: main→worker messages are delivered first,
    /// then worker→main replies/errors invoke `onmessage`/`onerror`/listeners on the main
    /// runtime. Returns the number of messages delivered.
    pub fn run_worker_tasks(&mut self, max_steps: usize) -> Result<usize, JsError> {
        let mut delivered = 0usize;
        for _ in 0..max_steps {
            let mut progressed = false;
            let ids: Vec<usize> = host_ref(&self.context)
                .expect("JavaScript host installed in context")
                .workers
                .iter()
                .enumerate()
                .filter_map(|(id, worker)| worker.as_ref().map(|_| id))
                .collect();
            for id in &ids {
                let mut state = host_cell(&mut self.context)
                    .expect("JavaScript host installed in context")
                    .workers[*id]
                    .take();
                let Some(mut worker) = state.take() else {
                    continue;
                };
                if worker.terminated {
                    // Free the slot so a new worker can reuse it (#1138).
                    host_cell(&mut self.context)
                        .expect("JavaScript host installed in context")
                        .workers[*id] = None;
                    continue;
                }
                while let Some(json) = worker.to_worker.pop_front() {
                    progressed = true;
                    delivered += 1;
                    if let Err(error) = worker.runtime.deliver_worker_message(&json) {
                        worker.pending_error = Some(error.to_string());
                    }
                }
                // F6-WORKERS-DOM: proxied DOM replies resolve Promises inside the worker;
                // drain its microtask queue so chained `then` callbacks run deterministically.
                if let Err(error) = worker.runtime.run_microtasks() {
                    worker.pending_error = Some(error.to_string());
                }
                if worker.pending_error.is_some() {
                    progressed = true;
                }
                host_cell(&mut self.context)
                    .expect("JavaScript host installed in context")
                    .workers[*id] = Some(worker);
            }
            for id in &ids {
                let reply = {
                    let mut host =
                        host_cell(&mut self.context).expect("JavaScript host installed in context");
                    host.workers[*id]
                        .as_mut()
                        .filter(|worker| !worker.terminated)
                        .and_then(|worker| {
                            host_cell(&mut worker.runtime.context)
                                .ok()?
                                .worker_outbox
                                .as_mut()?
                                .pop_front()
                        })
                };
                if let Some(json) = reply {
                    progressed = true;
                    delivered += 1;
                    // One worker's failing delivery must not starve the remaining
                    // workers in this pass (#1140).
                    if let Err(error) = self.deliver_worker_reply(*id as u32, &json) {
                        self.report_worker_delivery_error(error, "<worker message>");
                    }
                }
                let error = host_cell(&mut self.context)
                    .expect("JavaScript host installed in context")
                    .workers[*id]
                    .as_mut()
                    .and_then(|worker| worker.pending_error.take());
                if let Some(message) = error {
                    progressed = true;
                    delivered += 1;
                    if let Err(error) = self.deliver_worker_error(*id as u32, &message) {
                        self.report_worker_delivery_error(error, "<worker error>");
                    }
                }
            }
            if !progressed {
                break;
            }
        }
        Ok(delivered)
    }

    /// Reports a worker-delivery failure like any other uncaught error so the pump can
    /// continue with the other workers (#1140).
    pub(crate) fn report_worker_delivery_error(&mut self, error: JsError, source: &str) {
        let message = format!("{source}: {}", error);
        let _ = crate::events::report_uncaught(&mut self.context, &message, source);
    }

    pub(crate) fn deliver_worker_reply(&mut self, id: u32, json: &str) -> Result<(), JsError> {
        let event = self.parse_json_value(json)?;
        if self.handle_dom_proxy_call(id, &event)?.is_some() {
            return Ok(());
        }
        let onmessage = {
            let host = host_ref(&self.context).expect("JavaScript host installed in context");
            match host
                .workers
                .get(id as usize)
                .and_then(|worker| worker.as_ref())
            {
                Some(worker) => {
                    let mut slots = Vec::with_capacity(worker.onmessage.len() + 1);
                    if let Some(handler) = worker.onmessage_handler {
                        slots.push(handler);
                    }
                    slots.extend_from_slice(&worker.onmessage);
                    slots
                }
                None => return Ok(()),
            }
        };
        let event = self.parse_json_value(json)?;
        let mut init = ObjectInitializer::new(&mut self.context);
        init.property(js_string!("data"), event, Attribute::all());
        let event = JsValue::from(init.build());
        for slot in onmessage {
            if let Ok(callback) = root_get(
                &mut self.context,
                "__megi_worker_callbacks",
                u64::from(slot),
            ) && let Some(callback) = callback.as_object()
                && let Err(error) = callback.call(
                    &JsValue::undefined(),
                    std::slice::from_ref(&event),
                    &mut self.context,
                )
            {
                // Exceptions in worker message handlers must be reported, not
                // silently swallowed (#1460).
                self.report_worker_delivery_error(
                    JsError::Runtime(error.to_string()),
                    "<worker onmessage>",
                );
            }
        }
        Ok(())
    }

    pub(crate) fn deliver_worker_error(&mut self, id: u32, message: &str) -> Result<(), JsError> {
        let slot = {
            let host = host_ref(&self.context).expect("JavaScript host installed in context");
            host.workers
                .get(id as usize)
                .and_then(|worker| worker.as_ref())
                .and_then(|worker| worker.onerror)
        };
        let Some(slot) = slot else { return Ok(()) };
        let mut init = ObjectInitializer::new(&mut self.context);
        init.property(
            js_string!("message"),
            JsValue::from(js_string!(message.to_string())),
            Attribute::all(),
        )
        .property(
            js_string!("error"),
            JsValue::from(js_string!(message.to_string())),
            Attribute::all(),
        );
        let event = JsValue::from(init.build());
        if let Ok(callback) = root_get(
            &mut self.context,
            "__megi_worker_callbacks",
            u64::from(slot),
        ) && let Some(callback) = callback.as_object()
            && let Err(error) = callback.call(
                &JsValue::undefined(),
                std::slice::from_ref(&event),
                &mut self.context,
            )
        {
            self.report_worker_delivery_error(
                JsError::Runtime(error.to_string()),
                "<worker onerror>",
            );
        }
        Ok(())
    }

    pub(crate) fn parse_json_value(&mut self, json: &str) -> Result<JsValue, JsError> {
        let script = format!("JSON.parse({})", module_quote(json));
        self.context
            .eval(Source::from_bytes(&script))
            .map_err(|error| JsError::Runtime(error.to_string()))
    }
}

impl WorkerState {
    pub(crate) fn new(runtime: JsRuntime) -> Self {
        Self {
            runtime,
            terminated: false,
            to_worker: VecDeque::new(),
            dom_nodes: Vec::new(),
            onmessage: Vec::new(),
            onmessage_handler: None,
            onerror: None,
            pending_error: None,
        }
    }
}
