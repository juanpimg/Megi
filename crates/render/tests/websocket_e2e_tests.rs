//! #1534: end-to-end — a page script opens a real WebSocket, sends a message and receives the
//! echo, with the JS handler mutating the DOM (verified via the document title).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use net::websocket;
use render::{RenderOptions, Viewport, render_url};

fn spawn_echo_server() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
        let key = String::from_utf8_lossy(&head)
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(n, v)| {
                    if n.trim().eq_ignore_ascii_case("sec-websocket-key") {
                        Some(v.trim().to_owned())
                    } else {
                        None
                    }
                })
            })
            .expect("key");
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            websocket::accept_value(&key)
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        // Echo the first text frame, then exit (the test joins this thread).
        for _ in 0..1 {
            let mut header = [0u8; 2];
            if stream.read_exact(&mut header).is_err() {
                break;
            }
            let len = (header[1] & 0x7F) as usize;
            let mut rest = vec![0u8; 4 + len];
            if stream.read_exact(&mut rest).is_err() {
                break;
            }
            let mut payload = rest[4..].to_vec();
            websocket::mask(&mut payload, [rest[0], rest[1], rest[2], rest[3]]);
            let opcode = header[0] & 0x0F;
            if opcode == 0x1 {
                // text -> echo
                let mut echo = vec![0x81, len as u8];
                echo.extend_from_slice(&payload);
                if stream.write_all(&echo).is_err() {
                    break;
                }
                let _ = stream.flush();
            } else if opcode == 0x8 {
                // close
                break;
            }
        }
    });
    (port, handle)
}

#[test]
fn page_script_exchanges_messages_over_real_websocket() {
    let (port, server) = spawn_echo_server();
    let html = format!(
        "<html><head><title>pending</title></head><body><script>\
           var ws = new WebSocket('ws://127.0.0.1:{port}/echo'); \
           ws.onopen = function () {{ ws.send('hello-from-page'); }}; \
           ws.onmessage = function (e) {{ var p = document.createElement('p'); p.id = 'wsresult'; p.textContent = e.data; document.body.appendChild(p); }}; \
         </script></body></html>"
    );
    let path = std::env::temp_dir().join(format!(
        "megi-ws-e2e-{}-{}.html",
        std::process::id(),
        std::sync::atomic::AtomicUsize::new(0).fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, html).expect("write fixture");
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let url = format!("file://{}", path.display());
    let output = render_url(&url, &options).expect("render");
    let result = output
        .document
        .get_element_by_id("wsresult")
        .map(|node| output.document.text_content(node))
        .unwrap_or_default();
    assert_eq!(
        result, "hello-from-page",
        "WebSocket echo must reach the DOM: {result:?}"
    );
    let _ = std::fs::remove_file(&path);
    let _ = server.join();
}
