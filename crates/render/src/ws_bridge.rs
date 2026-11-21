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
const POLL_TIMEOUT: Duration = Duration::from_millis(10);

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
                        // #1638: `WebSocketClient::recv` already sent the single Pong reply.
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
                        // Yield briefly outside the mutex so send()/close() on the JS thread
                        // cannot be starved by the pump re-acquiring the lock immediately.
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => {
                        // #1825: a transport error also ends the pump; mark stop so the
                        // transport no longer reports itself as open.
                        pump_stop.store(true, Ordering::Relaxed);
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
}
