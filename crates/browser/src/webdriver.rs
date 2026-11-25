//! W3C WebDriver BiDi endpoint backed by real renders, Boa JS realms and shell input.
//!
//! Exposes a loopback-only HTTP `/status` endpoint and an RFC 6455 WebSocket channel at
//! `/session` (or `/session/<id>`) implementing `session`, `browsingContext`, `script`
//! and `input` commands over real `RenderOutput` documents and pixmaps.

mod browsing_context;
mod input;
mod script;

use std::collections::HashSet;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use megi_webview::render::{RenderOptions, RenderOutput};
use serde_json::{Value, json};

use crate::devtools::ws;
use crate::forms::{FormControl, TextEdit};
use crate::links::LinkRegion;

/// Live state for one WebDriver BiDi browsing context (tab).
pub(crate) struct BiDiContext {
    pub(crate) id: String,
    pub(crate) url: String,
    pub(crate) render: Option<RenderOutput>,
    pub(crate) forms: Vec<FormControl>,
    pub(crate) links: Vec<LinkRegion>,
    pub(crate) focused_form: Option<usize>,
    pub(crate) editor: Option<TextEdit>,
}

/// Active WebDriver BiDi session state shared across loopback client threads.
pub(crate) struct BiDiSession {
    active: bool,
    session_id: String,
    contexts: Vec<BiDiContext>,
    next_context_id: u64,
    next_nav_id: u64,
    subscriptions: HashSet<String>,
    pub(crate) options: RenderOptions,
}

impl BiDiSession {
    fn new(options: RenderOptions) -> Self {
        Self {
            active: false,
            session_id: "megi-bidi-1".to_owned(),
            contexts: Vec::new(),
            next_context_id: 1,
            next_nav_id: 1,
            subscriptions: HashSet::new(),
            options,
        }
    }

    pub(crate) fn start(&mut self, addr: SocketAddr) -> Value {
        self.active = true;
        if self.contexts.is_empty() {
            self.allocate_context();
        }
        json!({
            "sessionId": self.session_id,
            "capabilities": {
                "browserName": "megi",
                "browserVersion": env!("CARGO_PKG_VERSION"),
                "webSocketUrl": format!("ws://{addr}/session/{}", self.session_id),
            }
        })
    }

    pub(crate) fn end(&mut self) -> Value {
        self.active = false;
        self.contexts.clear();
        self.subscriptions.clear();
        json!({})
    }

    pub(crate) fn subscribe(&mut self, params: &Value) -> Value {
        if let Some(list) = params.get("events").and_then(Value::as_array) {
            for item in list {
                if let Some(name) = item.as_str() {
                    self.subscriptions.insert(name.to_owned());
                }
            }
        }
        json!({})
    }

    pub(crate) fn unsubscribe(&mut self, params: &Value) -> Value {
        if let Some(list) = params.get("events").and_then(Value::as_array) {
            for item in list {
                if let Some(name) = item.as_str() {
                    self.subscriptions.remove(name);
                }
            }
        }
        json!({})
    }

    pub(crate) fn is_subscribed(&self, event: &str) -> bool {
        self.subscriptions.contains(event)
    }

    pub(crate) fn context_tree_json(&self) -> Vec<Value> {
        self.contexts
            .iter()
            .map(|ctx| {
                json!({
                    "context": ctx.id,
                    "url": ctx.url,
                    "children": [],
                    "parent": Value::Null,
                })
            })
            .collect()
    }

    pub(crate) fn close_context(&mut self, id: &str) -> bool {
        if let Some(pos) = self.contexts.iter().position(|ctx| ctx.id == id) {
            self.contexts.remove(pos);
            true
        } else {
            false
        }
    }

    pub(crate) fn allocate_context(&mut self) -> &mut BiDiContext {
        let id = format!("context-{}", self.next_context_id);
        self.next_context_id += 1;
        self.contexts.push(BiDiContext {
            id,
            url: "about:blank".to_owned(),
            render: None,
            forms: Vec::new(),
            links: Vec::new(),
            focused_form: None,
            editor: None,
        });
        self.contexts.last_mut().expect("pushed context")
    }

    pub(crate) fn allocate_nav_id(&mut self) -> String {
        let id = format!("nav-{}", self.next_nav_id);
        self.next_nav_id += 1;
        id
    }

    pub(crate) fn find_context_mut(&mut self, id: &str) -> Option<&mut BiDiContext> {
        self.contexts.iter_mut().find(|ctx| ctx.id == id)
    }

    pub(crate) fn find_context(&self, id: &str) -> Option<&BiDiContext> {
        self.contexts.iter().find(|ctx| ctx.id == id)
    }
}

pub(crate) type SharedBiDi = Arc<Mutex<BiDiSession>>;

/// Handle for a running WebDriver BiDi server. Dropping it stops the accept loop.
#[derive(Debug)]
pub struct BiDiEndpoint {
    /// Actual bound loopback address (`127.0.0.1:<port>`).
    pub addr: SocketAddr,
    stop: Option<Sender<()>>,
}

impl Drop for BiDiEndpoint {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// Starts the WebDriver BiDi server on `127.0.0.1:<port>` (`0` selects an ephemeral port).
pub fn spawn(port: u16, options: RenderOptions) -> Result<BiDiEndpoint, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot set nonblocking: {error}"))?;
    let addr = listener
        .local_addr()
        .map_err(|error| format!("cannot read local address: {error}"))?;
    let (stop, stopped) = mpsc::channel::<()>();
    let state: SharedBiDi = Arc::new(Mutex::new(BiDiSession::new(options)));
    thread::Builder::new()
        .name("megi-webdriver-bidi".to_owned())
        .spawn(move || {
            while stopped.try_recv().is_err() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&state);
                        thread::spawn(move || serve_connection(stream, addr, &state));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| format!("cannot spawn WebDriver BiDi thread: {error}"))?;
    Ok(BiDiEndpoint {
        addr,
        stop: Some(stop),
    })
}

fn serve_connection(mut stream: TcpStream, addr: SocketAddr, state: &SharedBiDi) {
    let Ok(bytes) = ws::read_head(&mut stream) else {
        return;
    };
    let head = String::from_utf8_lossy(&bytes);
    let (method, path) = ws::parse_request_line(&head);
    if method != "GET" {
        ws::send_http(&mut stream, 405, b"method not allowed");
        return;
    }
    match path {
        "/status" => {
            let body = json!({
                "value": {
                    "ready": true,
                    "message": "Megi WebDriver BiDi ready",
                }
            });
            ws::send_http(&mut stream, 200, body.to_string().as_bytes());
        }
        "/session" => {
            ws::try_upgrade_websocket(&mut stream, &head, |payload| {
                dispatch_messages(payload, addr, state)
            });
        }
        _ if path.starts_with("/session/") => {
            ws::try_upgrade_websocket(&mut stream, &head, |payload| {
                dispatch_messages(payload, addr, state)
            });
        }
        _ => ws::send_http(&mut stream, 404, b"not found"),
    }
}

pub(crate) fn bidi_error(id: Value, error: &str, message: &str) -> String {
    json!({
        "id": id,
        "type": "error",
        "error": error,
        "message": message,
    })
    .to_string()
}

pub(crate) fn bidi_success(id: Value, result: Value) -> String {
    json!({
        "id": id,
        "type": "success",
        "result": result,
    })
    .to_string()
}

fn dispatch_messages(payload: &[u8], addr: SocketAddr, state: &SharedBiDi) -> Vec<String> {
    let request: Value = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(_) => return vec![bidi_error(Value::Null, "invalid argument", "invalid JSON")],
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return vec![bidi_error(id, "invalid argument", "missing method")];
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let mut session = state.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut events = Vec::new();

    let result = match method {
        "session.status" => Ok(json!({
            "ready": true,
            "message": "Megi WebDriver BiDi ready",
        })),
        "session.new" => Ok(session.start(addr)),
        "session.end" => Ok(session.end()),
        "session.subscribe" => Ok(session.subscribe(&params)),
        "session.unsubscribe" => Ok(session.unsubscribe(&params)),
        "browsingContext.create" => browsing_context::create(&mut session, &params),
        "browsingContext.getTree" => browsing_context::get_tree(&session, &params),
        "browsingContext.navigate" => {
            browsing_context::navigate(&mut session, &params, &mut events)
        }
        "browsingContext.reload" => browsing_context::reload(&mut session, &params, &mut events),
        "browsingContext.captureScreenshot" => {
            browsing_context::capture_screenshot(&mut session, &params)
        }
        "browsingContext.close" => browsing_context::close(&mut session, &params),
        "script.evaluate" => script::evaluate(&mut session, &params),
        "script.callFunction" => script::call_function(&mut session, &params),
        "input.performActions" => input::perform_actions(&mut session, &params),
        "input.releaseActions" => input::release_actions(&mut session, &params),
        _ => Err((
            "unknown command",
            format!("unsupported BiDi method {method}"),
        )),
    };

    let mut frames = events;
    frames.push(match result {
        Ok(value) => bidi_success(id, value),
        Err((code, msg)) => bidi_error(id, code, &msg),
    });
    frames
}
