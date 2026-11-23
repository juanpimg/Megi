//! Host-side bridge connecting the JS `WebSocket` API to the real `net` WebSocket client
//! (#1534). A pump thread drives non-blocking reads so `poll()` never blocks the JS event loop.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use megi_js::error::JsError;
use megi_js::host::{WebSocketEvent, WebSocketTransport};
use megi_net::http::FetchOptions;
use megi_net::websocket::{self, Opcode, WebSocketClient};

/// How long the pump thread blocks on each socket read before checking for stop/shutdown.
const POLL_TIMEOUT: Duration = Duration::from_millis(20);

/// w3-render-005: hard cap on buffered WebSocket events. Without it a remote server
/// flooding frames while the JS consumer is busy grows the queue without bound.
const MAX_QUEUED_WS_EVENTS: usize = 1024;

/// A live WebSocket transport backed by a real TCP connection.
pub struct LiveWebSocketTransport {
    client: Arc<Mutex<WebSocketClient>>,
    events: mpsc::Receiver<WebSocketEvent>,
    stop: Arc<AtomicBool>,
    /// w3-render-005: number of events dropped because the bounded queue was full.
    dropped: Arc<AtomicUsize>,
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
        // w3-render-005: use a bounded channel so a remote server flooding frames cannot
        // grow the queue without bound while the JS consumer is busy. `try_send` below
        // drops excess events and records an aggregate overflow count.
        let (tx, events) = mpsc::sync_channel(MAX_QUEUED_WS_EVENTS);
        // The handshake completed, so report the connection as open immediately. This mirrors
        // the browser behaviour that fires `open` before any `message` handler can run.
        let _ = tx.send(WebSocketEvent::Open);
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicUsize::new(0));
        let pump_client = Arc::clone(&client);
        let pump_stop = Arc::clone(&stop);
        let pump_dropped = Arc::clone(&dropped);
        // Helper: send an event, counting it as dropped (with a single aggregate signal)
        // when the bounded queue is full or the receiver is gone.
        fn push_event(
            tx: &mpsc::SyncSender<WebSocketEvent>,
            dropped: &AtomicUsize,
            overflow_signalled: &mut bool,
            event: WebSocketEvent,
        ) -> bool {
            match tx.try_send(event) {
                Ok(()) => true,
                Err(mpsc::TrySendError::Full(_)) => {
                    dropped.fetch_add(1, Ordering::Relaxed);
                    if !*overflow_signalled {
                        // One aggregate "messages dropped" signal; best effort, never block.
                        let _ = tx.try_send(WebSocketEvent::Error(
                            "WebSocket messages dropped: event queue full".to_owned(),
                        ));
                        *overflow_signalled = true;
                    }
                    true
                }
                Err(mpsc::TrySendError::Disconnected(_)) => false,
            }
        }
        std::thread::spawn(move || {
            let mut overflow_signalled = false;
            while !pump_stop.load(Ordering::Relaxed) {
                let result = lock(&pump_client).recv();
                match result {
                    Ok(frame) => match frame.opcode {
                        // #1711: binary frames must keep their exact bytes; only Text frames
                        // are decoded as UTF-8 strings.
                        Opcode::Text => {
                            let text = String::from_utf8_lossy(&frame.payload).into_owned();
                            if !push_event(
                                &tx,
                                &pump_dropped,
                                &mut overflow_signalled,
                                WebSocketEvent::Message(text),
                            ) {
                                break;
                            }
                        }
                        Opcode::Binary => {
                            if !push_event(
                                &tx,
                                &pump_dropped,
                                &mut overflow_signalled,
                                WebSocketEvent::Binary(frame.payload),
                            ) {
                                break;
                            }
                        }
                        // #1638 / RFC 6455 §5.5.2: WebSocketClient::reassemble() already
                        // sends exactly one Pong for every Ping it surfaces via recv().
                        // Sending another one here produced duplicate Pongs on the wire.
                        Opcode::Ping | Opcode::Pong => {}
                        Opcode::Close => {
                            let code = if frame.payload.len() >= 2 {
                                u16::from_be_bytes([frame.payload[0], frame.payload[1]])
                            } else {
                                websocket::close_code::NO_STATUS
                            };
                            // #1825: a remote close ends the connection; stop the pump so
                            // is_open() reports the truth and the socket can be released.
                            pump_stop.store(true, Ordering::Relaxed);
                            let _ = tx.try_send(WebSocketEvent::Close(code));
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
                        // Yield outside the client mutex so JS send()/close() calls are never
                        // starved by tight pump re-locking under CI CPU load.
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        continue; // no frame yet; loop and re-check `stop`
                    }
                    Err(error) => {
                        // #1825: a transport error also ends the pump; mark stop so the
                        // transport no longer reports itself as open.
                        pump_stop.store(true, Ordering::Relaxed);
                        let _ = tx.try_send(WebSocketEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            client,
            events,
            stop,
            dropped,
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
        let mut events: Vec<WebSocketEvent> = self.events.try_iter().collect();
        // w3-render-005: if the queue overflowed, surface an aggregate count so the
        // JS side can observe that messages were lost (reset for the next drain).
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            events.push(WebSocketEvent::Error(format!(
                "WebSocket messages dropped: {dropped} events (queue full)"
            )));
        }
        events
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = lock(&self.client).close(websocket::close_code::NORMAL);
    }
}

// #1825: dropping a live transport must stop its receive pump, otherwise a render
// with an open WebSocket leaks the pump thread and the socket.
impl Drop for LiveWebSocketTransport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = lock(&self.client).close(websocket::close_code::NORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Completes a WS handshake on a loopback socket, then sends a Close frame. The pump
    /// must observe the remote Close, set `stop`, and make `is_open()` return false (#1825).
    #[test]
    fn remote_close_stops_the_pump() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("timeout");
            let mut bytes = Vec::new();
            let mut byte = [0u8; 1];
            while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 16 * 1024 {
                match stream.read(&mut byte) {
                    Ok(1) => bytes.push(byte[0]),
                    _ => break,
                }
            }
            let request = String::from_utf8_lossy(&bytes).into_owned();
            let key = request
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                            Some(value.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                websocket::accept_value(&key)
            );
            stream
                .write_all(response.as_bytes())
                .expect("write handshake");
            stream.flush().expect("flush");
            std::thread::sleep(Duration::from_millis(50));
            // Server-to-client Close frame (unmasked), code 1000.
            let frame = websocket::Frame {
                fin: true,
                opcode: Opcode::Close,
                masked: false,
                payload: websocket::close_code::NORMAL.to_be_bytes().to_vec(),
            };
            let bytes = websocket::encode_frame(&frame, [0; 4]);
            stream.write_all(&bytes).expect("write close");
            let _ = stream.flush();
            std::thread::sleep(Duration::from_millis(200));
        });

        let url = format!("ws://127.0.0.1:{port}/chat");
        let options = FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        };
        let transport = LiveWebSocketTransport::connect(&url, &options).expect("connect");
        assert!(
            transport.is_open(),
            "pump must be running right after connect"
        );
        // Wait for the pump to observe the remote Close frame and set `stop`.
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while transport.is_open() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !transport.is_open(),
            "a remote Close must stop the pump (#1825)"
        );
        let _ = server.join();
    }

    /// w3-render-005: a remote server flooding frames must not grow the event queue
    /// without bound while the JS consumer is busy. The pump must cap the queue and
    /// surface an aggregate "messages dropped" signal instead of buffering everything.
    #[test]
    fn flooding_frames_cannot_grow_the_event_queue_without_bound() {
        const FLOOD: usize = 2 * MAX_QUEUED_WS_EVENTS + 64;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut bytes = Vec::new();
            let mut byte = [0u8; 1];
            while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 16 * 1024 {
                match stream.read(&mut byte) {
                    Ok(1) => bytes.push(byte[0]),
                    _ => break,
                }
            }
            let request = String::from_utf8_lossy(&bytes).into_owned();
            let key = request
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                            Some(value.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                websocket::accept_value(&key)
            );
            stream.write_all(response.as_bytes()).expect("handshake");
            let _ = stream.flush();
            // Send far more frames than the queue can hold, as fast as possible.
            let frame = websocket::Frame {
                fin: true,
                opcode: Opcode::Text,
                masked: false,
                payload: b"x".to_vec(),
            };
            let encoded = websocket::encode_frame(&frame, [0; 4]);
            for _ in 0..FLOOD {
                if stream.write_all(&encoded).is_err() {
                    break;
                }
            }
            let _ = stream.flush();
            std::thread::sleep(Duration::from_millis(100));
        });

        let url = format!("ws://127.0.0.1:{port}/flood");
        let options = FetchOptions {
            connect_timeout: Duration::from_secs(2),
            read_timeout: Duration::from_secs(2),
            ..FetchOptions::default()
        };
        let mut transport = LiveWebSocketTransport::connect(&url, &options).expect("connect");
        // Let the pump receive frames without draining via poll().
        std::thread::sleep(Duration::from_millis(300));
        // Draining must yield a bounded number of events, never one per frame.
        let drained = WebSocketTransport::poll(&mut transport);
        assert!(
            drained.len() <= MAX_QUEUED_WS_EVENTS + 2,
            "event queue must be bounded (got {} events, cap {MAX_QUEUED_WS_EVENTS})",
            drained.len()
        );
        // The overflow must be signalled rather than silently dropped.
        assert!(
            drained.iter().any(|event| matches!(
                event,
                WebSocketEvent::Error(message) if message.contains("dropped")
            )),
            "an overflowed queue must report dropped messages"
        );
        transport.close();
        let _ = server.join();
    }
}
