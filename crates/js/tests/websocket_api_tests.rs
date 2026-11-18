//! #1534: JS WebSocket class over a host-injected transport.

#![cfg(feature = "js")]

use std::collections::VecDeque;
use std::sync::Arc;

use megi_js::error::JsError;
use megi_js::host::{WebSocketEvent, WebSocketProvider, WebSocketTransport};
use megi_js::runtime::JsRuntime;

struct MockTransport {
    events: VecDeque<WebSocketEvent>,
    sent: Vec<String>,
}

impl WebSocketTransport for MockTransport {
    fn send(&mut self, text: &str) -> Result<(), JsError> {
        self.sent.push(text.to_owned());
        Ok(())
    }
    fn poll(&mut self) -> Vec<WebSocketEvent> {
        self.events.drain(..).collect()
    }
    fn close(&mut self) {}
}

fn runtime_with(
    events: Vec<WebSocketEvent>,
) -> (JsRuntime, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
    let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&sent);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: WebSocketProvider = Arc::new(move |_url| {
        Ok(Box::new(MockTransport {
            events: VecDeque::from(events.clone()),
            sent: Vec::new(),
        }))
    });
    runtime.set_websocket_provider(provider);
    let _ = seen;
    (runtime, sent)
}

#[test]
fn websocket_class_lifecycle_and_events() {
    let events = vec![
        WebSocketEvent::Open,
        WebSocketEvent::Message("hello".to_owned()),
        WebSocketEvent::Close(1000),
    ];
    let (mut runtime, _sent) = runtime_with(events);
    runtime
        .eval(
            "globalThis.__log = []; \
         var ws = new WebSocket('ws://example.com/chat'); \
         ws.onopen = function () { __log.push('open:' + ws.readyState); }; \
         ws.onmessage = function (e) { __log.push('msg:' + e.data); }; \
         ws.onclose = function (e) { __log.push('close:' + e.code + ':' + ws.readyState); }; \
         String([typeof ws.send, typeof ws.close, WebSocket.OPEN, WebSocket.CLOSED].join(','));",
            "t",
        )
        .expect("construct");
    let delivered = runtime.poll_websockets();
    assert_eq!(delivered, 1, "one instance should receive events");
    let log = runtime.eval("__log.join('|')", "t").expect("log");
    assert_eq!(log, "open:1|msg:hello|close:1000:3", "lifecycle: {log}");
}

#[test]
fn websocket_send_and_constants() {
    let (mut runtime, _sent) = runtime_with(vec![WebSocketEvent::Open]);
    let out = runtime
        .eval(
            "var ws = new WebSocket('ws://example.com'); \
             String([WebSocket.CONNECTING, WebSocket.OPEN, WebSocket.CLOSING, WebSocket.CLOSED].join(','));",
            "t",
        )
        .expect("constants");
    assert_eq!(out, "0,1,2,3");
    runtime.poll_websockets(); // => OPEN
    runtime.eval("ws.send('ping')", "t").expect("send");
    runtime.eval("ws.close()", "t").expect("close");
}
