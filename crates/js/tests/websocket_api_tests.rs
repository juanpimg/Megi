//! #1534: JS WebSocket class over a host-injected transport.

#![cfg(feature = "js")]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use megi_js::error::JsError;
use megi_js::host::{WebSocketEvent, WebSocketProvider, WebSocketTransport};
use megi_js::runtime::JsRuntime;

type SentLog = Arc<Mutex<Vec<String>>>;

struct MockTransport {
    events: VecDeque<WebSocketEvent>,
    sent: SentLog,
}

impl WebSocketTransport for MockTransport {
    fn send(&mut self, text: &str) -> Result<(), JsError> {
        self.sent
            .lock()
            .expect("sent log poisoned")
            .push(text.to_owned());
        Ok(())
    }
    fn poll(&mut self) -> Vec<WebSocketEvent> {
        self.events.drain(..).collect()
    }
    fn close(&mut self) {}
}

fn runtime_with(events: Vec<WebSocketEvent>) -> (JsRuntime, SentLog) {
    let sent: SentLog = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&sent);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: WebSocketProvider = Arc::new(move |_url| {
        Ok(Box::new(MockTransport {
            events: VecDeque::from(events.clone()),
            sent: Arc::clone(&seen),
        }))
    });
    runtime.set_websocket_provider(provider);
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
    let (mut runtime, sent) = runtime_with(vec![WebSocketEvent::Open]);
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
    assert_eq!(
        sent.lock().expect("sent log poisoned").as_slice(),
        ["ping"],
        "send() must deliver its payload to the transport"
    );
}

/// #1711: a Binary frame must reach the JS `message` handler as an `ArrayBuffer`
/// (not a lossy UTF-8 string), preserving the exact bytes.
#[test]
fn websocket_binary_message_is_delivered_as_arraybuffer() {
    let bytes = vec![0x00, 0xFF, 0x80, 0x41];
    let events = vec![
        WebSocketEvent::Open,
        WebSocketEvent::Binary(bytes.clone()),
        WebSocketEvent::Close(1000),
    ];
    let (mut runtime, _sent) = runtime_with(events);
    runtime
        .eval(
            "globalThis.__log = []; \
             var ws = new WebSocket('ws://example.com/bin'); \
             ws.onmessage = function (e) { \
               __log.push(e.data instanceof ArrayBuffer ? 'ab' : typeof e.data); \
               if (e.data instanceof ArrayBuffer) { \
                 __log.push(Array.from(new Uint8Array(e.data)).join(',')); \
               } \
             }; \
             String(typeof ws.onmessage);",
            "t",
        )
        .expect("construct");
    runtime.poll_websockets();
    let log = runtime.eval("__log.join('|')", "t").expect("log");
    assert_eq!(
        log, "ab|0,255,128,65",
        "binary message must be an ArrayBuffer with exact bytes: {log}"
    );
}

/// #1847: close() must keep the transport alive until poll() delivers the Close
/// event, so `onclose` fires and readyState reaches CLOSED (3).
#[test]
fn websocket_close_fires_onclose_and_reaches_closed_state() {
    // A transport whose Close event only becomes available on the poll AFTER close().
    use std::sync::atomic::{AtomicBool, Ordering};

    struct DeferredCloseTransport {
        close_called: Arc<AtomicBool>,
        emitted: bool,
    }
    impl WebSocketTransport for DeferredCloseTransport {
        fn send(&mut self, _text: &str) -> Result<(), JsError> {
            Ok(())
        }
        fn poll(&mut self) -> Vec<WebSocketEvent> {
            if self.close_called.load(Ordering::SeqCst) && !self.emitted {
                self.emitted = true;
                vec![WebSocketEvent::Close(1000)]
            } else {
                Vec::new()
            }
        }
        fn close(&mut self) {
            self.close_called.store(true, Ordering::SeqCst);
        }
    }

    let close_called = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&close_called);
    let mut runtime = JsRuntime::new().expect("runtime");
    let provider: WebSocketProvider = Arc::new(move |_url| {
        Ok(Box::new(DeferredCloseTransport {
            close_called: Arc::clone(&flag),
            emitted: false,
        }))
    });
    runtime.set_websocket_provider(provider);

    runtime
        .eval(
            "globalThis.__log = []; \
             var ws = new WebSocket('ws://example.com'); \
             ws.onopen = function () { __log.push('open:' + ws.readyState); }; \
             ws.onclose = function (e) { __log.push('close:' + e.code + ':' + ws.readyState); }; \
             ws.onmessage = function () {}; \
             String(ws.readyState);",
            "t",
        )
        .expect("construct");

    // First poll delivers the synthetic Open so the socket is usable, then close().
    // (Our transport emits no Open, so emulate readiness by forcing state via the
    // native path is unnecessary: close() works from CONNECTING too per the prelude.)
    runtime.poll_websockets();
    runtime.eval("ws.close()", "t").expect("close");
    assert!(
        close_called.load(Ordering::SeqCst),
        "transport.close() must be called"
    );

    // The Close event arrives on the NEXT poll. With the bug, the transport entry was
    // removed at close() time, so this poll delivers nothing and onclose never fires.
    let delivered = runtime.poll_websockets();
    assert_eq!(delivered, 1, "close event must be delivered after close()");
    let log = runtime.eval("__log.join('|')", "t").expect("log");
    assert_eq!(
        log, "close:1000:3",
        "onclose must fire with readyState CLOSED: {log}"
    );
    let state = runtime.eval("String(ws.readyState)", "t").expect("state");
    assert_eq!(state, "3", "readyState must be CLOSED after the event");
}
