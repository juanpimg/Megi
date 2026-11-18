//! w2-render-01 RED: global iframe budget (fan-out 4, depth 4 must stay <= 16 renders).
//! Counts real document loads through a loopback HTTP server: each `render_url_inner`
//! fetches its document URL, so the request count equals the total render count.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use megi_render::{RenderOptions, ScriptingMode, Viewport, render_url};

#[test]
fn frame_fanout_global_budget() {
    // Self-referential page: every document contains 4 iframes pointing at itself.
    // Total renders without a global budget: 1 + 4 + 16 + 64 + 256 = 341.
    let page = Arc::new(std::sync::Mutex::new(String::new()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    *page.lock().unwrap() = format!(
        "<html><body style='margin:0'>\
         <iframe width='20' height='10' src='http://{addr}/page.html'></iframe>\
         <iframe width='20' height='10' src='http://{addr}/page.html'></iframe>\
         <iframe width='20' height='10' src='http://{addr}/page.html'></iframe>\
         <iframe width='20' height='10' src='http://{addr}/page.html'></iframe>\
         </body></html>"
    );
    listener.set_nonblocking(true).expect("nonblocking server");
    let count = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let server_page = page.clone();
    let server_count = count.clone();
    let server_done = done.clone();
    let handle = std::thread::spawn(move || {
        while !server_done.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf);
                    server_count.fetch_add(1, Ordering::SeqCst);
                    let body = server_page.lock().unwrap().clone();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let url = format!("http://{addr}/page.html");
    let options = RenderOptions {
        viewport: Viewport::new(200, 150).expect("viewport"),
        scripting: ScriptingMode::Disabled,
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render");
    done.store(true, Ordering::Relaxed);
    let _ = handle.join();
    let total = count.load(Ordering::SeqCst);
    // Sanity: the render itself succeeded.
    assert!(
        !output.png.is_empty() || output.pixmap.width > 0,
        "render must produce a pixmap"
    );
    // `total` includes the top-level document; frames are everything else.
    let frames = total.saturating_sub(1);
    assert!(
        frames <= megi_render::frames::MAX_FRAMES_PER_PAGE,
        "global frame budget exceeded: {frames} frame renders ({total} document loads) for a 4-wide self-nested page (cap {})",
        megi_render::frames::MAX_FRAMES_PER_PAGE
    );
    // The cap must surface as a warning on the top-level page.
    assert!(
        output.stats.warnings.iter().any(|w| w.contains("(cap)")),
        "budget exhaustion must warn: {:?}",
        output.stats.warnings
    );
}
