//! #1538: form POST navigation sends the urlencoded body through the net loader.
//!
//! A loopback server captures the full request (head + body) and echoes it into the rendered
//! document, so the test proves the method/Content-Type/body reached the wire.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[cfg(feature = "js")]
use render::ScriptingMode;
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
                                let (name, value) = line.split_once(':')?;
                                name.trim()
                                    .eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap_or(0))
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

#[test]
#[cfg(feature = "js")]
fn page_js_form_data_fetch_posts_multipart_to_server() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base_url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let seen = Arc::clone(&captured);

    let server = std::thread::spawn(move || {
        // Request 1: GET /page.html; Request 2: POST /submit
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            for _ in 0..16 {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        req.extend_from_slice(&buf[..n]);
                        if let Some(end) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                            let cl = String::from_utf8_lossy(&req[..end])
                                .lines()
                                .find_map(|l| {
                                    let (k, v) = l.split_once(':')?;
                                    k.trim()
                                        .eq_ignore_ascii_case("content-length")
                                        .then(|| v.trim().parse::<usize>().ok())
                                        .flatten()
                                })
                                .unwrap_or(0);
                            if req.len() >= end + 4 + cl {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let req_text = String::from_utf8_lossy(&req);
            if req_text.starts_with("GET /page.html") {
                let html = concat!(
                    "<!doctype html><html><body>",
                    "<form id='f'>",
                    "<input name='user' value='alice'>",
                    "<input type='checkbox' name='agree' checked>",
                    "</form>",
                    "<p id='out'></p>",
                    "<script>",
                    "var fd = new FormData(document.getElementById('f'));",
                    "fd.append('extra', '42');",
                    "fetch('/submit', { method: 'POST', body: fd })",
                    ".then(function(r){ return r.text(); })",
                    ".then(function(t){ document.getElementById('out').textContent = t; });",
                    "</script></body></html>"
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(resp.as_bytes());
            } else {
                *seen.lock().expect("lock") = req;
                let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 7\r\nConnection: close\r\n\r\ncreated";
                let _ = stream.write_all(resp.as_bytes());
            }
            let _ = stream.flush();
        }
    });

    let options = RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    };
    let output = render_url(&base_url, &options).expect("render page with FormData fetch");
    assert_eq!(output.stats.status, Some(200));

    let raw = captured.lock().expect("lock").clone();
    let req_str = String::from_utf8_lossy(&raw);
    assert!(
        req_str.starts_with("POST /submit HTTP/1.1"),
        "expected POST /submit: {req_str}"
    );
    assert!(
        req_str
            .to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary=----megiformboundary"),
        "expected multipart/form-data Content-Type: {req_str}"
    );
    assert!(
        req_str.contains("Content-Disposition: form-data; name=\"user\"\r\n\r\nalice\r\n"),
        "expected user field in multipart body: {req_str}"
    );
    assert!(
        req_str.contains("Content-Disposition: form-data; name=\"agree\"\r\n\r\non\r\n"),
        "expected agree checkbox default 'on' in multipart body: {req_str}"
    );
    assert!(
        req_str.contains("Content-Disposition: form-data; name=\"extra\"\r\n\r\n42\r\n"),
        "expected appended extra field in multipart body: {req_str}"
    );
    server.join().expect("server thread");
}
