//! M7 foundation: DevTools protocol endpoint in the browser shell.
//!
//! DevTools Protocol discovery (Chrome CDP / Firefox remote debugging) serves
//! `GET /json/version` and `GET /json/list` over HTTP, then a WebSocket target that
//! exchanges JSON-RPC messages (`Runtime.evaluate`). The bind is loopback-only and the
//! endpoint is opt-in via `--remote-debugging-port`.
//!
//! WebSocket framing and the upgrade digest reuse `megi-net` via `megi-webview::net`:
//! RFC 6455 §4.2.2 defines `Sec-WebSocket-Accept`, §5.1 requires masked client frames,
//! §5.2 requires unmasked server frames. The endpoint stays loopback-only and opt-in.
//! Commands are dispatched in the `protocol` module: with a real page attached, DOM/CSS commands
//! read the live arena (WHATWG DOM §4.2.3 / CSSOM §6) and the Network domain replays the
//! loader's real request/response log and the Performance domain exports the pipeline's
//! real `RenderTimings`; without a page, evaluation falls back to the isolated realm.
//! Multiple targets and WebDriver BiDi are still out of scope.

mod network;
mod performance;
mod protocol;
pub(crate) mod ws;

pub use protocol::DevToolsPage;
use protocol::{SharedPage, lock_page, rpc_messages};

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

const TARGET_ID: &str = "page-1";

/// DevTools target advertised by `/json/list` (title and URL come from the CLI).
#[derive(Clone, Debug)]
pub struct DebugTarget {
    pub title: String,
    pub url: String,
}

/// Handle for the running endpoint. Dropping it signals the accept loop to stop.
#[derive(Debug)]
pub struct DebugEndpoint {
    /// Actual bound address (`127.0.0.1:<port>`); port 0 selects an ephemeral port.
    pub addr: SocketAddr,
    stop: Option<Sender<()>>,
    page: SharedPage,
}

impl Drop for DebugEndpoint {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

/// Starts the DevTools endpoint on `127.0.0.1:<port>` (port 0 = ephemeral, for tests).
///
/// No page is attached yet: `Runtime.evaluate` uses the isolated realm and DOM/CSS
/// commands report that no target is loaded. The shell attaches the real document once a
/// render succeeds ([`DebugEndpoint::attach`]).
pub fn spawn(port: u16, target: DebugTarget) -> Result<DebugEndpoint, String> {
    spawn_with_page(port, target, None)
}

/// Starts the endpoint with an already loaded page attached.
pub fn spawn_with_page(
    port: u16,
    target: DebugTarget,
    page: Option<DevToolsPage>,
) -> Result<DebugEndpoint, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("cannot set nonblocking: {error}"))?;
    let addr = listener
        .local_addr()
        .map_err(|error| format!("cannot read local address: {error}"))?;
    let (stop, stopped) = mpsc::channel::<()>();
    let page: SharedPage = Arc::new(Mutex::new(page));
    let accept_page = Arc::clone(&page);
    thread::Builder::new()
        .name("megi-devtools".to_owned())
        .spawn(move || {
            while stopped.try_recv().is_err() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let target = target.clone();
                        let addr = addr;
                        let page = Arc::clone(&accept_page);
                        thread::spawn(move || serve_connection(stream, addr, &target, &page));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| format!("cannot spawn DevTools thread: {error}"))?;
    Ok(DebugEndpoint {
        addr,
        stop: Some(stop),
        page,
    })
}

impl DebugEndpoint {
    /// Attaches a loaded page (or replaces the previous attachment after a navigation).
    pub fn attach(&self, page: DevToolsPage) {
        *lock_page(&self.page) = Some(page);
    }
}

fn serve_connection(stream: TcpStream, addr: SocketAddr, target: &DebugTarget, page: &SharedPage) {
    let mut stream = stream;
    let bytes = match ws::read_head(&mut stream) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    let head = String::from_utf8_lossy(&bytes);
    let (method, path) = ws::parse_request_line(&head);
    if method != "GET" {
        ws::send_http(&mut stream, 405, b"method not allowed");
        return;
    }
    match path {
        "/json/version" => {
            ws::send_http(&mut stream, 200, version_json(addr).to_string().as_bytes());
        }
        "/json/list" => ws::send_http(
            &mut stream,
            200,
            list_json(addr, target).to_string().as_bytes(),
        ),
        _ if path.starts_with("/devtools/page/") => {
            ws::try_upgrade_websocket(&mut stream, &head, |payload| rpc_messages(payload, page));
        }
        _ => ws::send_http(&mut stream, 404, b"not found"),
    }
}

fn version_json(addr: SocketAddr) -> Value {
    let agent = format!("Megi/{}", env!("CARGO_PKG_VERSION"));
    json!({
        "Browser": agent,
        "Protocol-Version": "1.3",
        "User-Agent": agent,
        "webSocketDebuggerUrl": format!("ws://{addr}/devtools/page/{TARGET_ID}"),
    })
}

fn list_json(addr: SocketAddr, target: &DebugTarget) -> Value {
    json!([{
        "id": TARGET_ID,
        "type": "page",
        "title": target.title,
        "url": target.url,
        "webSocketDebuggerUrl": format!("ws://{addr}/devtools/page/{TARGET_ID}"),
    }])
}
