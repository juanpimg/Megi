//! #1538: form POST navigation sends the urlencoded body through the net loader.
//!
//! A loopback server captures the full request (head + body) and echoes it into the rendered
//! document, so the test proves the method/Content-Type/body reached the wire.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use render::{RenderOptions, Viewport, render_url};

fn spawn_echo() -> (String, Arc<Mutex<Vec<u8>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let url = format!("http://{}/submit", listener.local_addr().expect("addr"));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&captured);
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = Vec::new();
        let mut buffer = [0u8; 512];
        // Read until we have headers + the 11-byte body, with a bounded number of reads.
        for _ in 0..16 {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    request.extend_from_slice(&buffer[..read]);
                    let head_end = request.windows(4).position(|w| w == b"\r\n\r\n");
                    if let Some(end) = head_end {
                        let content_length = String::from_utf8_lossy(&request[..end])
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("Content-Length: ")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + content_length {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        *seen.lock().expect("lock") = request.clone();
        let body = String::from_utf8_lossy(&request)
            .replace('\r', "")
            .replace('\n', "\\n");
        let html = format!("<!doctype html><html><body><p id=req>{body}</p></body></html>");
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            html.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(html.as_bytes());
        let _ = stream.flush();
    });
    (url, captured, handle)
}

#[test]
fn form_post_sends_urlencoded_body_with_content_type() {
    let (url, captured, server) = spawn_echo();
    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        form_post: Some((
            "application/x-www-form-urlencoded; charset=UTF-8".to_owned(),
            b"user=alice&remember=1".to_vec(),
        )),
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render POST response");
    assert_eq!(output.stats.status, Some(200));

    let request = captured.lock().expect("lock").clone();
    let head = String::from_utf8_lossy(&request);
    assert!(
        head.starts_with("POST /submit HTTP/1.1"),
        "request must be POST: {head}"
    );
    assert!(
        head.contains("content-type: application/x-www-form-urlencoded")
            || head.contains("Content-Type: application/x-www-form-urlencoded"),
        "POST must carry the urlencoded content type: {head}"
    );
    assert!(
        request.ends_with(b"user=alice&remember=1"),
        "POST body must be the urlencoded payload: {head}"
    );
    server.join().expect("server thread");
}
