//! Host-side bridge connecting the JS `WebSocket` API to the real `net` WebSocket client
//! (#1534). A pump thread drives non-blocking reads so `poll()` never blocks the JS event loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use megi_js::error::JsError;
use megi_js::host::{WebSocketEvent, WebSocketTransport};
use megi_net::http::FetchOptions;
use megi_net::websocket::{self, Opcode, WebSocketClient};

/// How long the pump thread blocks on each socket read before checking for stop/shutdown.
const POLL_TIMEOUT: Duration = Duration::from_millis(20);

/// A live WebSocket transport backed by a real TCP connection.
pub struct LiveWebSocketTransport {
    client: Arc<Mutex<WebSocketClient>>,
    events: mpsc::Receiver<WebSocketEvent>,
    stop: Arc<AtomicBool>,
}

fn lock(client: &Mutex<WebSocketClient>) -> std::sync::MutexGuard<'_, WebSocketClient> {
    client
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl LiveWebSocketTransport {
    /// Opens a `ws://` or `wss://` connection and starts its receive pump. `wss://` handshakes
    /// are performed over TLS (no silent downgrade to plaintext).
    pub fn connect(url: &str, options: &FetchOptions) -> Result<Self, JsError> {
        let parsed = websocket::parse_ws_url(url)
            .map_err(|error| JsError::Runtime(format!("WebSocket URL: {error}")))?;
        let mut client = WebSocketClient::connect(&parsed, options)
            .map_err(|error| JsError::Runtime(format!("WebSocket handshake: {error}")))?;
        client
            .set_read_timeout(POLL_TIMEOUT)
            .map_err(|error| JsError::Runtime(format!("WebSocket timeout: {error}")))?;
        let client = Arc::new(Mutex::new(client));
        let (tx, events) = mpsc::channel();
        // The handshake completed, so report the connection as open immediately. This mirrors
        // the browser behaviour that fires `open` before any `message` handler can run.
        let _ = tx.send(WebSocketEvent::Open);
        let stop = Arc::new(AtomicBool::new(false));
        let pump_client = Arc::clone(&client);
        let pump_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !pump_stop.load(Ordering::Relaxed) {
                let result = lock(&pump_client).recv();
                match result {
                    Ok(frame) => match frame.opcode {
                        // #1711: binary frames must keep their exact bytes; only Text frames
                        // are decoded as UTF-8 strings.
                        Opcode::Text => {
                            let text = String::from_utf8_lossy(&frame.payload).into_owned();
                            if tx.send(WebSocketEvent::Message(text)).is_err() {
                                break;
                            }
                        }
                        Opcode::Binary => {
                            if tx.send(WebSocketEvent::Binary(frame.payload)).is_err() {
                                break;
                            }
                        }
                        Opcode::Ping => {
                            let _ = lock(&pump_client).pong();
                        }
                        Opcode::Pong => {}
                        Opcode::Close => {
                            let code = if frame.payload.len() >= 2 {
                                u16::from_be_bytes([frame.payload[0], frame.payload[1]])
                            } else {
                                websocket::close_code::NO_STATUS
                            };
                            let _ = tx.send(WebSocketEvent::Close(code));
                            break;
                        }
                        Opcode::Continuation => {}
                    },
                    Err(megi_net::FetchError::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue; // no frame yet; loop and re-check `stop`
                    }
                    Err(error) => {
                        let _ = tx.send(WebSocketEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            client,
            events,
            stop,
        })
    }

    /// Reports whether the receive pump is still running (used by diagnostics/tests).
    pub fn is_open(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
}

impl WebSocketTransport for LiveWebSocketTransport {
    fn send(&mut self, text: &str) -> Result<(), JsError> {
        lock(&self.client)
            .send_text(text)
            .map_err(|error| JsError::Runtime(format!("WebSocket send: {error}")))
    }

    fn poll(&mut self) -> Vec<WebSocketEvent> {
        self.events.try_iter().collect()
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = lock(&self.client).close(websocket::close_code::NORMAL);
    }
}
