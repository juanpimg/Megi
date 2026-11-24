//! #2249: a CDN error page served as HTTP 200 must produce a warning.

use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn access_denied_http_200_is_flagged() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut head = [0u8; 4096];
        let _ = stream.read(&mut head).expect("read request");
        let body = b"<HTML><HEAD><TITLE>Access Denied</TITLE></HEAD><BODY>\
                     <H1>Access Denied</H1><P>You don't have permission.</P></BODY></HTML>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).expect("head");
        stream.write_all(body).expect("body");
    });

    let dir = std::env::temp_dir().join(format!("megi-errorpage-{port}"));
    let _ = std::fs::create_dir_all(&dir);
    let options = megi_render::RenderOptions {
        viewport: megi_render::Viewport::new(400, 300).expect("viewport"),
        ..megi_render::RenderOptions::default()
    };
    let out =
        megi_render::render_url(&format!("http://127.0.0.1:{port}/"), &options).expect("render");
    server.join().expect("server");

    assert!(
        out.stats.warnings.iter().any(|w| w.contains("error page")),
        "expected an error-page warning, got: {:?}",
        out.stats.warnings
    );
}
