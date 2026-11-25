//! `WebSocket` built on a host-injected transport (#1534).
//!
//! The host owns the socket (via `megi_net::websocket::WebSocketClient`); this module installs a
//! native `__megi_ws_native(url, op, payload)` bridge and a small JS `WebSocket` class with the
//! standard readyState constants, `send`/`close` and `onopen`/`onmessage`/`onclose`/`onerror`
//! handlers. Events are delivered when the host calls `JsRuntime::poll_websockets()`.

use boa_engine::{
    Context, JsResult, JsValue, NativeFunction, Source, js_string, object::FunctionObjectBuilder,
};

use crate::error::JsError;

/// Native-to-JS marker for a typed quota failure (#2311). The prelude turns this
/// into a `DOMException` named `QuotaExceededError`; keep both strings in sync.
const WS_LIMIT_PREFIX: &str = "WebSocketLimitError: ";

const WEBSOCKET_PRELUDE: &str = r####"
(function () {
  var g = typeof window !== "undefined" ? window : globalThis;

  function WebSocket(url) {
    if (typeof url !== "string" && typeof url !== "object") {
      throw new TypeError("WebSocket URL must be a string");
    }
    var target = String(url);
    var self = this;
    this.url = target;
    this.readyState = 0; // CONNECTING
    this.bufferedAmount = 0;
    this._listeners = {};

    // #1747: register `on<event>` handlers as listeners at assignment time
    // (HTML §6.1.6.2), so they fire in registration order relative to
    // addEventListener and participate in the same listener dispatch.
    var WS_HANDLER_TYPES = ['open', 'message', 'close', 'error'];
    (function (obj) {
      for (var i = 0; i < WS_HANDLER_TYPES.length; i++) (function (type) {
        var stored = null;
        Object.defineProperty(obj, 'on' + type, {
          configurable: true,
          enumerable: true,
          get: function () { return stored; },
          set: function (fn) {
            // Replace the previous property handler in the listener list.
            if (stored !== null) {
              obj._listeners[type] = (obj._listeners[type] || []).filter(function (f) {
                return f !== stored;
              });
            }
            stored = typeof fn === 'function' ? fn : null;
            if (stored === null) { return; }
            if (!obj._listeners[type]) { obj._listeners[type] = []; }
            if (obj._listeners[type].indexOf(stored) < 0) { obj._listeners[type].push(stored); }
          }
        });
      })(WS_HANDLER_TYPES[i]);
    })(this);

    this.addEventListener = function (type, fn) {
      if (typeof fn !== "function") { return; }
      if (!self._listeners[type]) { self._listeners[type] = []; }
      if (self._listeners[type].length < 64) { self._listeners[type].push(fn); }
    };
    this.removeEventListener = function (type, fn) {
      var list = self._listeners[type] || [];
      self._listeners[type] = list.filter(function (f) { return f !== fn; });
    };
    this._fire = function (type, event) {
      // #1747: `on<event>` handlers are registered as listeners above, so they
      // fire here in registration order; do not invoke the property handler first.
      var list = (self._listeners[type] || []).slice();
      for (var i = 0; i < list.length; i++) { try { list[i](event); } catch (e) {} }
    };

    if (typeof __megi_ws_native !== "function") {
      self.readyState = 3; // CLOSED
      var err = { type: "error", message: "no WebSocket provider" };
      setTimeout(function () { self._fire("error", err); }, 0);
      return;
    }
    self._handle = __megi_ws_native(target, "open", "");
    if (typeof self._handle !== "number" || self._handle < 0) {
      self.readyState = 3;
      var rawError = typeof self._handle === "string" ? self._handle : "WebSocket connection failed";
      var errMsg = rawError;
      var typedError = null;
      if (rawError.indexOf("WebSocketLimitError: ") === 0) {
        // #2311: the connection cap is a quota failure, not an opaque message.
        // Hand scripts a typed DOMException they can branch on by `name`.
        errMsg = rawError.slice("WebSocketLimitError: ".length);
        if (typeof DOMException === "function") {
          typedError = new DOMException(errMsg, "QuotaExceededError");
        } else {
          typedError = new Error(errMsg);
          typedError.name = "QuotaExceededError";
        }
      }
      var err2 = {
        type: "error", message: errMsg,
        name: typedError ? typedError.name : "", error: typedError
      };
      setTimeout(function () { self._fire("error", err2); }, 0);
      return;
    }
    // Register this instance so poll_websockets can deliver events to it.
    if (!globalThis.__megi_ws_instances) { globalThis.__megi_ws_instances = {}; }
    globalThis.__megi_ws_instances[self._handle] = self;
  }

  WebSocket.CONNECTING = 0;
  WebSocket.OPEN = 1;
  WebSocket.CLOSING = 2;
  WebSocket.CLOSED = 3;

  WebSocket.prototype.send = function (data) {
    if (this.readyState === 0) { throw new Error("InvalidStateError: WebSocket is still connecting"); }
    if (this.readyState >= 2) { return; }
    if (typeof __megi_ws_native === "function" && typeof this._handle === "number") {
      // #2153: pass raw BufferSource values to native without coercing through String(data).
      __megi_ws_native(this.url, "send:" + this._handle, data);
    }
  };
  WebSocket.prototype.close = function (code, reason) {
    if (this.readyState >= 2) { return; }
    this.readyState = 2; // CLOSING
    if (typeof __megi_ws_native === "function" && typeof this._handle === "number") {
      __megi_ws_native(this.url, "close:" + this._handle, String(code === undefined ? 1000 : code));
    }
  };

  // Called by the host (JsRuntime::poll_websockets) to deliver pending events.
  globalThis.__megi_ws_dispatch = function (handle, events) {
    var self = globalThis.__megi_ws_instances && globalThis.__megi_ws_instances[handle];
    if (!self) { return; }
    for (var i = 0; i < events.length; i++) {
      var e = events[i];
      if (e[0] === "open") {
        self.readyState = 1;
        self._fire("open", { type: "open", target: self });
      } else if (e[0] === "message") {
        self._fire("message", { type: "message", data: e[1], target: self });
      } else if (e[0] === "close") {
        self.readyState = 3;
        self._fire("close", { type: "close", code: e[1], target: self });
        if (globalThis.__megi_ws_instances) { delete globalThis.__megi_ws_instances[handle]; }
      } else if (e[0] === "error") {
        self._fire("error", { type: "error", message: e[1], target: self });
      }
    }
  };

  g.WebSocket = WebSocket;
})();
"####;

enum WsSendPayload {
    Text(String),
    Binary(Vec<u8>),
}

fn ws_native(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let string = |index: usize| -> String {
        args.get(index)
            .and_then(|value| value.as_string())
            .map(|text| text.to_std_string_escaped())
            .unwrap_or_default()
    };
    let url = string(0);
    let op = string(1);

    if op == "open" {
        // #2311 / RFC 6455 §4.1: enforce the per-document cap *before* invoking the
        // provider. Each successful open registers a transport and, in the real
        // host, starts a receive thread holding an FD; the provider may allocate
        // both eagerly, so checking afterwards would already be too late.
        let (limit, live) = match crate::host::host_cell(context) {
            Ok(host) => (host.max_websockets, host.websockets.len()),
            Err(error) => return Ok(JsValue::from(js_string!(error.to_string()))),
        };
        if live >= limit {
            return Ok(JsValue::from(js_string!(format!(
                "{WS_LIMIT_PREFIX}WebSocket connection limit exceeded ({limit} per document)"
            ))));
        }
        // Clone the Arc from the immutable provider slot so the provider runs with
        // no active host borrow (a transport factory may re-enter JavaScript).
        let provider = crate::providers::providers(context)
            .and_then(|slot| slot.websocket.clone())
            .or_else(|| {
                crate::host::host_cell(context)
                    .ok()
                    .and_then(|host| host.websocket_provider.clone())
            });
        let opened = match provider {
            None => Err("no WebSocket provider".to_owned()),
            Some(provider) => match provider(&url).map_err(|error| error.to_string()) {
                Ok(transport) => match crate::host::host_cell(context) {
                    Ok(mut host) => {
                        let handle = host.websocket_next_handle;
                        host.websocket_next_handle = handle.wrapping_add(1);
                        host.websockets.insert(handle, transport);
                        Ok(handle)
                    }
                    Err(error) => Err(error.to_string()),
                },
                Err(message) => Err(message),
            },
        };
        return match opened {
            Ok(handle) => Ok(JsValue::from(handle as f64)),
            Err(message) => Ok(JsValue::from(js_string!(message))),
        };
    }

    // send:/close:<handle>
    let (command, handle_str) = op.split_once(':').unwrap_or((&op, ""));
    let handle = handle_str.parse::<u32>().unwrap_or(0);
    let send_payload = if command == "send" {
        let raw = args.get(2).cloned().unwrap_or_else(JsValue::undefined);
        if let Some(obj) = raw.as_object()
            && let Some(bytes) =
                crate::runtime::buffer_source::extract_buffer_source_bytes(&obj, context)?
        {
            Some(WsSendPayload::Binary(bytes))
        } else {
            let text = raw.to_string(context)?.to_std_string_escaped();
            Some(WsSendPayload::Text(text))
        }
    } else {
        None
    };
    if let Ok(mut host) = crate::host::host_cell(context)
        && let Some(transport) = host.websockets.get_mut(&handle)
    {
        match command {
            "send" => {
                if let Some(payload) = send_payload {
                    match payload {
                        WsSendPayload::Text(text) => {
                            let _ = transport.send(&text);
                        }
                        WsSendPayload::Binary(bytes) => {
                            let _ = transport.send_binary(&bytes);
                        }
                    }
                }
            }
            "close" => {
                // #1847: keep the transport registered so poll() can deliver the
                // Close event (and the JS prelude can set readyState=3 / fire
                // onclose). The entry is removed in poll() when Close is delivered.
                transport.close();
            }
            _ => {}
        }
    }
    Ok(JsValue::undefined())
}

/// Installs the WebSocket bridge and JS class into `context`.
pub fn install(context: &mut Context) -> Result<(), JsError> {
    let native =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(ws_native))
            .name(js_string!("__megi_ws_native"))
            .length(3)
            .build();
    context
        .global_object()
        .set(
            js_string!("__megi_ws_native"),
            JsValue::from(native),
            false,
            context,
        )
        .map_err(|error| JsError::Internal(error.to_string()))?;
    context
        .eval(Source::from_bytes(WEBSOCKET_PRELUDE))
        .map(|_| ())
        .map_err(|error| JsError::Internal(error.to_string()))?;
    Ok(())
}

/// Polls every live transport and dispatches its events to the matching JS instance.
pub fn poll(context: &mut Context) -> usize {
    use crate::host::WebSocketEvent;

    let events: Vec<(u32, Vec<WebSocketEvent>)> = {
        let Ok(mut host) = crate::host::host_cell(context) else {
            return 0;
        };
        let handles: Vec<u32> = host.websockets.keys().copied().collect();
        let mut collected = Vec::new();
        for handle in handles {
            if let Some(transport) = host.websockets.get_mut(&handle) {
                let events = transport.poll();
                if !events.is_empty() {
                    collected.push((handle, events));
                }
            }
        }
        collected
    };

    let mut dispatched = 0;
    for (handle, events) in events {
        // #1847: once a Close event is observed, retire the transport from the host
        // so it is polled no further. The JS prelude sets readyState=3 and fires
        // onclose when it receives this event.
        let delivered_close = events
            .iter()
            .any(|event| matches!(event, WebSocketEvent::Close(_)));
        let args: Vec<JsValue> = events
            .into_iter()
            .map(|event| {
                use boa_engine::object::builtins::{AlignedVec, JsArray, JsArrayBuffer};
                let parts: Vec<JsValue> = match event {
                    WebSocketEvent::Open => vec![js_string!("open").into()],
                    WebSocketEvent::Message(text) => vec![
                        js_string!("message").into(),
                        js_string!(text.as_str()).into(),
                        js_string!("text").into(),
                    ],
                    // #1711: deliver the exact bytes as an ArrayBuffer, never a lossy string.
                    WebSocketEvent::Binary(bytes) => {
                        let block = AlignedVec::from_iter(0, bytes.iter().copied());
                        let Ok(buffer) = JsArrayBuffer::from_byte_block(block, context) else {
                            return vector(context, &["error", "binary message too large"]);
                        };
                        vec![
                            js_string!("message").into(),
                            JsValue::from(buffer),
                            js_string!("binary").into(),
                        ]
                    }
                    // #2152: WHATWG WebSockets — CloseEvent.code is an unsigned short (number).
                    WebSocketEvent::Close(code) => {
                        vec![js_string!("close").into(), JsValue::from(f64::from(code))]
                    }
                    WebSocketEvent::Error(message) => vec![
                        js_string!("error").into(),
                        js_string!(message.as_str()).into(),
                    ],
                };
                JsValue::from(JsArray::from_iter(parts, context))
            })
            .collect();
        let array = boa_engine::object::builtins::JsArray::from_iter(args, context);
        let dispatch = context
            .global_object()
            .get(js_string!("__megi_ws_dispatch"), context)
            .ok()
            .and_then(|value| value.as_callable());
        if let Some(function) = dispatch {
            let _ = function.call(
                &JsValue::undefined(),
                &[JsValue::from(handle as f64), JsValue::from(array)],
                context,
            );
            dispatched += 1;
        }
        if delivered_close {
            // #1847: retire the closed transport now that its Close event was delivered.
            if let Ok(mut host) = crate::host::host_cell(context) {
                host.websockets.remove(&handle);
            }
        }
    }
    dispatched
}

fn vector(context: &mut Context, parts: &[&str]) -> JsValue {
    let array = boa_engine::object::builtins::JsArray::from_iter(
        parts.iter().map(|part| JsValue::from(js_string!(*part))),
        context,
    );
    JsValue::from(array)
}
