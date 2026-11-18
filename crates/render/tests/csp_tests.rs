//! #1526: Content Security Policy — `script-src 'none'` blocks inline and external scripts.
//! Self-contained loopback HTTP server serving the page with a CSP header.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use render::{RenderOptions, ScriptingMode, Viewport, render_url};

fn options() -> RenderOptions {
    RenderOptions {
        viewport: Viewport::new(320, 240).expect("viewport"),
        scripting: ScriptingMode::Enabled,
        ..RenderOptions::default()
    }
}

fn serve_page(page: String) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    (url, handle)
}

#[test]
fn csp_none_blocks_inline_script() {
    let page = "<html><body><p id=x>plain</p><script>document.getElementById('x').textContent='hacked'</script></body></html>".to_owned();
    let (url, server) = serve_page(page);
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 0,
        "CSP script-src 'none' must block the inline script"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("Content-Security-Policy")),
        "a warning must explain the CSP block: {:?}",
        output.stats.warnings
    );
    server.join().expect("server");
}

#[test]
fn csp_unsafe_inline_allows_inline_script() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let page =
        "<html><body><script>document.body.setAttribute('data-ok','1')</script></body></html>"
            .to_owned();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 1,
        "'unsafe-inline' must allow the inline script"
    );
    handle.join().expect("server");
}

#[test]
fn csp_meta_tag_and_multiple_header_intersection() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    // Header allows 'unsafe-inline', but <meta> intersects with script-src 'none'.
    let page = r#"<html><head>
        <meta http-equiv="Content-Security-Policy" content="script-src 'none'">
    </head><body><script>document.body.setAttribute('data-hacked','1')</script></body></html>"#;
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'\r\nContent-Security-Policy: default-src 'self' 'unsafe-inline'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        output.stats.scripts_executed, 0,
        "intersection of HTTP header + <meta> CSP must block the script"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("Content-Security-Policy")),
        "expected CSP warning: {:?}",
        output.stats.warnings
    );
    handle.join().expect("server");
}

#[test]
fn csp_style_src_blocks_disallowed_inline_and_external_styles() {
    let blocked_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_clone = std::sync::Arc::clone(&blocked_hits);

    let css_listener = TcpListener::bind("127.0.0.1:0").expect("bind css");
    let css_port = css_listener.local_addr().expect("addr").port();
    let _css_handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = css_listener.accept() {
            hits_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: 26\r\n\r\nbody{background:#ff0000;}");
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let page = format!(
        "<html><head>\
        <style>body{{background:#ff0000}}</style>\
        <style nonce=\"ok\">body{{background:#00ff00}}</style>\
        <link rel=\"stylesheet\" href=\"http://127.0.0.1:{css_port}/evil.css\">\
        </head><body></body></html>"
    );
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: style-src 'nonce-ok'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(
        blocked_hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "external stylesheet blocked by CSP style-src must not be fetched"
    );
    assert_eq!(
        output.stats.inline_stylesheets, 1,
        "only the nonced <style> should apply"
    );
    let px = output.pixmap.pixel(10, 10);
    assert_eq!(
        (px.r, px.g, px.b, px.a),
        (0, 255, 0, 255),
        "nonced <style> green background must win while un-nonced red <style> is blocked"
    );
    handle.join().expect("server");
}

#[test]
fn csp_img_src_blocks_disallowed_images() {
    let img_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_clone = std::sync::Arc::clone(&img_hits);
    let img_listener = TcpListener::bind("127.0.0.1:0").expect("bind img");
    let img_port = img_listener.local_addr().expect("addr").port();
    let _img_handle = std::thread::spawn(move || {
        if let Ok((_stream, _)) = img_listener.accept() {
            hits_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let page = format!(
        "<html><head><meta http-equiv=\"Content-Security-Policy\" content=\"img-src 'self'\"></head>\
        <body><img src=\"http://127.0.0.1:{img_port}/pic.png\"></body></html>"
    );
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    assert_eq!(output.stats.images_loaded, 0);
    assert_eq!(output.stats.images_failed, 1);
    assert_eq!(
        img_hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "blocked img-src must never connect to third-party image host"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("Content-Security-Policy")),
        "expected CSP image warning: {:?}",
        output.stats.warnings
    );
    handle.join().expect("server");
}

#[test]
fn csp_connect_src_and_unsafe_eval_enforced_in_js() {
    let api_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits_clone = std::sync::Arc::clone(&api_hits);
    let api_listener = TcpListener::bind("127.0.0.1:0").expect("bind api");
    let api_port = api_listener.local_addr().expect("addr").port();
    let _api_handle = std::thread::spawn(move || {
        if let Ok((_stream, _)) = api_listener.accept() {
            hits_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page");
    let url = format!("http://{}/page.html", listener.local_addr().expect("addr"));
    let page = format!(
        r#"<html><body><script>
        try {{
            eval('1 + 1');
            console.log('eval-allowed');
        }} catch (e) {{
            console.log('eval-blocked:' + e.name);
        }}
        fetch('http://127.0.0.1:{api_port}/data').then(
            () => console.log('fetch-allowed'),
            () => console.log('fetch-blocked')
        );
        try {{
            var x = new XMLHttpRequest();
            x.open('GET', 'http://127.0.0.1:{api_port}/xhr', false);
            x.send();
            console.log('xhr-status:' + x.status);
        }} catch (e) {{
            console.log('xhr-blocked');
        }}
        </script></body></html>"#
    );
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Security-Policy: script-src 'unsafe-inline'; connect-src 'self'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.len(),
            page
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });
    let output = render_url(&url, &options()).expect("render");
    let logs: Vec<&str> = output
        .stats
        .console
        .iter()
        .map(|entry| entry.message.as_str())
        .collect();
    assert!(
        logs.contains(&"eval-blocked:EvalError"),
        "missing 'unsafe-eval' must block eval() with EvalError: {logs:?}"
    );
    assert!(
        logs.contains(&"fetch-blocked"),
        "connect-src 'self' must reject cross-origin fetch: {logs:?}"
    );
    assert!(
        logs.contains(&"xhr-blocked"),
        "connect-src 'self' must block cross-origin XHR: {logs:?}"
    );
    assert_eq!(
        api_hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "connect-src-blocked requests must not hit network"
    );
    handle.join().expect("server");
}
