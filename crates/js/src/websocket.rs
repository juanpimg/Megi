//! `WebSocket` built on a host-injected transport (#1534).
//!
//! The host owns the socket (via `net::websocket::WebSocketClient`); this module installs a
//! native `__megi_ws_native(url, op, payload)` bridge and a small JS `WebSocket` class with the
//! standard readyState constants, `send`/`close` and `onopen`/`onmessage`/`onclose`/`onerror`
//! handlers. Events are delivered when the host calls `JsRuntime::poll_websockets()`.

use boa_engine::{
    Context, JsResult, JsValue, NativeFunction, Source, js_string, object::FunctionObjectBuilder,
};

use crate::error::JsError;

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
    this.onopen = null;
    this.onmessage = null;
    this.onclose = null;
    this.onerror = null;
    this._listeners = {};

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
      var handler = self["on" + type];
      if (typeof handler === "function") { try { handler(event); } catch (e) {} }
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
      var errMsg = typeof self._handle === "string" ? self._handle : "WebSocket connection failed";
      var err2 = { type: "error", message: errMsg };
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
      __megi_ws_native(this.url, "send:" + this._handle, String(data));
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

fn ws_native(_this: &JsValue, args: &[JsValue], _context: &mut Context) -> JsResult<JsValue> {
    let string = |index: usize| -> String {
        args.get(index)
            .and_then(|value| value.as_string())
            .map(|text| text.to_std_string_escaped())
            .unwrap_or_default()
    };
    let url = string(0);
    let op = string(1);
    let payload = string(2);

    if op == "open" {
        let opened = crate::host::with_host(|host| -> Result<u32, String> {
            let Some(provider) = host.websocket_provider.as_ref() else {
                return Err("no WebSocket provider".to_owned());
            };
            let transport = provider(&url).map_err(|error| error.to_string())?;
            let handle = host.websocket_next_handle;
            host.websocket_next_handle = host.websocket_next_handle.wrapping_add(1);
            host.websockets.insert(handle, transport);
            Ok(handle)
        })
        .unwrap_or_else(|| Err("no active host".to_owned()));
        return match opened {
            Ok(handle) => Ok(JsValue::from(handle as f64)),
            Err(message) => Ok(JsValue::from(js_string!(message))),
        };
    }

    // send:/close:<handle>
    let (command, handle_str) = op.split_once(':').unwrap_or((&op, ""));
    let handle = handle_str.parse::<u32>().unwrap_or(0);
    crate::host::with_host(|host| {
        if let Some(transport) = host.websockets.get_mut(&handle) {
            match command {
                "send" => {
                    let _ = transport.send(&payload);
                }
                "close" => {
                    transport.close();
                    host.websockets.remove(&handle);
                }
                _ => {}
            }
        }
    });
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
        crate::host::with_host(|host| {
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
        })
        .unwrap_or_default()
    };

    let mut dispatched = 0;
    for (handle, events) in events {
        let args: Vec<JsValue> = events
            .into_iter()
            .map(|event| match event {
                WebSocketEvent::Open => vector(context, &["open"]),
                WebSocketEvent::Message(text) => vector(context, &["message", &text]),
                WebSocketEvent::Close(code) => vector(context, &["close", &code.to_string()]),
                WebSocketEvent::Error(message) => vector(context, &["error", &message]),
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
