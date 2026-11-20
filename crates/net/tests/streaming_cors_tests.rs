//! #1855: CORS must be checked before a streaming body reaches `on_data`.
//!
//! The buffered `fetch()` path validates CORS before returning the body. The streaming
//! path used to hand every response chunk to `on_data` while reading, and only validate
//! CORS afterwards — so a cross-origin response without ACAO was fully delivered to the
//! consumer before the `Err(Cors)`. That makes the CORS policy ineffective (Fetch §4.10).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::{FetchError, FetchOptions, Url, http};

fn spawn_body_server(body: &'static [u8]) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => request.extend_from_slice(&buffer[..read]),
            }
        }
        // No Access-Control-Allow-Origin: cross-origin CORS must reject this response.
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    });
    (port, handle)
}

fn cross_origin_options() -> FetchOptions {
    FetchOptions {
        origin: Some("https://app.example".to_owned()),
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

#[test]
fn streaming_validates_cors_before_delivering_the_body() {
    let (port, server) = spawn_body_server(b"SECRET-CROSS-ORIGIN-BODY");
    let url = Url::parse(&format!("http://127.0.0.1:{port}/secret")).expect("url");
    let mut delivered = Vec::new();
    let error = http::fetch_streaming(&url, &cross_origin_options(), &mut |chunk| {
        delivered.extend_from_slice(chunk);
        Ok(())
    })
    .expect_err("cross-origin response without ACAO must fail CORS");
    server.join().expect("server thread");
    assert!(matches!(error, FetchError::Cors(_)), "{error}");
    assert!(
        delivered.is_empty(),
        "no body bytes may reach on_data before CORS passes, got {:?}",
        String::from_utf8_lossy(&delivered)
    );
}
