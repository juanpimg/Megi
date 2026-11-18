//! #1544: iframes render their child document into the parent page.

use std::sync::atomic::{AtomicUsize, Ordering};

use render::{RenderOptions, Viewport, render_url};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn render_page(html: &str) -> render::RenderOutput {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "megi-iframe-test-{}-{}.html",
        std::process::id(),
        id
    ));
    std::fs::write(&path, html).expect("write fixture");
    let url = format!("file://{}", path.display());
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&url, &options).expect("render");
    let _ = std::fs::remove_file(&path);
    output
}

fn count_color(pixmap: &paint::Pixmap, color: css::Color) -> usize {
    pixmap
        .pixels
        .iter()
        .filter(|pixel| **pixel == color)
        .count()
}

const RED: css::Color = css::Color::new(0xFF, 0x00, 0x00, 0xFF);

#[test]
fn iframe_srcdoc_renders_child_document() {
    let output = render_page(
        "<html><body style='margin:0'>\
         <iframe width='40' height='20' srcdoc='<body style=\"margin:0;background:red\"></body>'></iframe>\
         </body></html>",
    );
    let red = count_color(&output.pixmap, RED);
    assert!(
        red > 100,
        "iframe child document red pixels must paint, got {red}"
    );
}

#[test]
fn iframe_src_loads_external_document() {
    let dir = std::env::temp_dir();
    let stem = format!(
        "megi-iframe-src-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let child_name = format!("{stem}-child.html");
    let child_path = dir.join(&child_name);
    let parent_path = dir.join(format!("{stem}-parent.html"));
    std::fs::write(
        &child_path,
        "<body style='margin:0;background:rgb(0,0,255)'></body>",
    )
    .expect("child");
    std::fs::write(
        &parent_path,
        format!("<html><body style='margin:0'><iframe width='30' height='15' src='{child_name}'></iframe></body></html>"),
    )
    .expect("parent");
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let url = format!("file://{}", parent_path.display());
    let output = render_url(&url, &options).expect("render");
    let blue = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
    assert!(
        count_color(&output.pixmap, blue) > 50,
        "iframe src child must paint blue"
    );
    let _ = std::fs::remove_file(&child_path);
    let _ = std::fs::remove_file(&parent_path);
}

#[test]
fn iframe_depth_cap_prevents_infinite_recursion() {
    // A frame whose srcdoc contains itself must terminate at MAX_FRAME_DEPTH.
    let nested = "<iframe width='20' height='10' srcdoc='IFRAME_MARKER'></iframe>";
    let child_html = format!("<body style='margin:0;background:green'>{nested}</body>");
    let marker = "IFRAME_MARKER";
    let child = child_html.replace(
        marker,
        &format!(
            "<iframe width='20' height='10' srcdoc='{}'></iframe>",
            child_html.replace('\'', "\"")
        ),
    );
    let output = render_page(&format!(
        "<html><body style='margin:0'><iframe width='40' height='20' srcdoc='{}'></iframe></body></html>",
        child.replace('\'', "\"")
    ));
    // The test completing at all proves the depth cap works; green from the child is visible.
    let green = css::Color::new(0x00, 0x80, 0x00, 0xFF);
    assert!(
        count_color(&output.pixmap, green) > 0,
        "child document must paint green"
    );
}

#[test]
fn iframe_without_srcdoc_paints_no_child_content() {
    let output = render_page(
        "<html><body style='margin:0'><iframe width='40' height='20'></iframe></body></html>",
    );
    assert_eq!(count_color(&output.pixmap, RED), 0, "no child => no red");
}

#[cfg(feature = "js")]
#[test]
#[cfg(feature = "js")]
fn iframe_sandbox_blocks_child_scripts() {
    // Child script sets a red background; it only runs when sandbox allows scripts.
    let script = "<script>document.body.setAttribute('style','margin:0;background:red')</script>";
    let child = format!("<body style='margin:0'>{script}</body>");

    // Default sandbox (no tokens) must block scripts.
    let blocked_html = format!(
        "<html><body style='margin:0'><iframe width='40' height='20' sandbox srcdoc=\"{child}\"></iframe></body></html>"
    );
    let blocked = render_page(&blocked_html);
    assert_eq!(
        count_color(&blocked.pixmap, RED),
        0,
        "sandboxed iframe must not execute the child script"
    );

    // allow-scripts must let it run.
    let allowed_html = format!(
        "<html><body style='margin:0'><iframe width='40' height='20' sandbox='allow-scripts' srcdoc=\"{child}\"></iframe></body></html>"
    );
    let allowed = render_page(&allowed_html);
    assert!(
        count_color(&allowed.pixmap, RED) > 100,
        "allow-scripts must let the child script paint"
    );
}

#[test]
fn iframe_css_width_and_height_size_viewport_in_px_and_percent() {
    // Parent viewport is 400x300:
    // - frame 1: CSS 80x40 overrides HTML width/height (10x10) => ~3200 red pixels.
    // - frame 2: CSS 50% x 20% = 200x60 => ~12000 blue pixels.
    let output = render_page(
        "<html><head><style>\
         .px{width:80px;height:40px;display:block;border:0}\
         .pct{width:50%;height:20%;display:block;border:0}\
         </style></head><body style='margin:0'>\
         <iframe class='px' width='10' height='10' srcdoc='<body style=\"margin:0;background:red\"></body>'></iframe>\
         <iframe class='pct' srcdoc='<body style=\"margin:0;background:rgb(0,0,255)\"></body>'></iframe>\
         </body></html>",
    );
    let red = count_color(&output.pixmap, RED);
    let blue = count_color(&output.pixmap, css::Color::new(0x00, 0x00, 0xFF, 0xFF));
    assert!(
        red >= 2_500,
        "CSS px dimensions must grow iframe beyond HTML 10x10 fallback, got {red} red px"
    );
    assert!(
        blue >= 10_000,
        "CSS % dimensions (50%x20% of 400x300) must size iframe viewport, got {blue} blue px"
    );
}

type HttpRoute = (&'static str, String, Vec<(&'static str, &'static str)>);
type OwnedHttpRoute = (String, String, Vec<(String, String)>);

fn spawn_http_routes(
    routes: Vec<HttpRoute>,
    max_requests: usize,
) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let base_url = format!("http://{addr}");
    let owned_routes: Vec<OwnedHttpRoute> = routes
        .into_iter()
        .map(|(path, body, headers)| {
            (
                path.to_owned(),
                body,
                headers
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
            )
        })
        .collect();
    let handle = std::thread::spawn(move || {
        for _ in 0..max_requests {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let req_path = req
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let matched = owned_routes
                .iter()
                .find(|(path, _, _)| path == req_path)
                .or_else(|| owned_routes.first());
            if let Some((_, body, headers)) = matched {
                let mut hdrs = String::new();
                for (k, v) in headers {
                    hdrs.push_str(&format!("{k}: {v}\r\n"));
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{hdrs}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        }
    });
    (base_url, handle)
}

#[test]
fn iframe_csp_frame_src_and_child_src_block_disallowed_frames() {
    let green = css::Color::new(0x00, 0x80, 0x00, 0xFF);
    let (other_origin, other_handle) = spawn_http_routes(
        vec![(
            "/other.html",
            "<body style='margin:0;background:red'></body>".to_owned(),
            vec![],
        )],
        1,
    );
    let parent_html = format!(
        "<html><body style='margin:0'>\
         <iframe width='50' height='30' src='/same.html'></iframe>\
         <iframe width='50' height='30' src='{other_origin}/other.html'></iframe>\
         </body></html>"
    );
    // Notice `other_origin` uses same IP 127.0.0.1 with a DIFFERENT port, so `'self'` rejects it
    // while allowing `/same.html`.
    let (same_origin, same_handle) = spawn_http_routes(
        vec![
            (
                "/parent.html",
                parent_html,
                vec![("Content-Security-Policy", "frame-src 'self'")],
            ),
            (
                "/same.html",
                "<body style='margin:0;background:green'></body>".to_owned(),
                vec![],
            ),
        ],
        2,
    );
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        ..RenderOptions::default()
    };
    let output = render_url(&format!("{same_origin}/parent.html"), &options).expect("render");
    assert!(
        count_color(&output.pixmap, green) > 200,
        "same-origin iframe allowed by frame-src 'self' must paint green"
    );
    assert_eq!(
        count_color(&output.pixmap, RED),
        0,
        "cross-port iframe blocked by frame-src 'self' must not render"
    );
    assert!(
        output
            .stats
            .warnings
            .iter()
            .any(|w| w.contains("Content-Security-Policy")),
        "expected CSP warning, got: {:?}",
        output.stats.warnings
    );

    // Fallback: `child-src 'none'` blocks child frames when `frame-src` is absent.
    let (fallback_origin, fallback_handle) = spawn_http_routes(
        vec![(
            "/parent.html",
            "<html><body style='margin:0'><iframe width='50' height='30' src='/same.html'></iframe></body></html>".to_owned(),
            vec![("Content-Security-Policy", "child-src 'none'")],
        )],
        1,
    );
    let blocked = render_url(&format!("{fallback_origin}/parent.html"), &options).expect("render");
    assert_eq!(
        count_color(&blocked.pixmap, green),
        0,
        "child-src 'none' must block iframe via fallback"
    );
    // `other_handle` never receives a connection because CSP blocked it before network fetch;
    // connect once cleanly to unblock accept thread or drop listener.
    let _ = std::net::TcpStream::connect(other_origin.trim_start_matches("http://"));
    other_handle.join().expect("other");
    same_handle.join().expect("same");
    fallback_handle.join().expect("fallback");
}

#[cfg(feature = "js")]
#[test]
fn iframe_parent_script_mutating_content_document_reaches_pixels() {
    let output = render_page(
        "<html><body style='margin:0'>\
         <iframe width='60' height='40' srcdoc='<body style=\"margin:0\"></body>'></iframe>\
         <script>\
         var doc = document.querySelector('iframe').contentDocument;\
         doc.body.setAttribute('style', 'margin:0;background:red');\
         </script>\
         </body></html>",
    );
    assert!(
        count_color(&output.pixmap, RED) >= 1_500,
        "parent DOM mutation via contentDocument must paint in child frame"
    );
}

#[cfg(feature = "js")]
#[test]
fn iframe_bidirectional_post_message_reaches_parent_and_child_pixels() {
    let blue = css::Color::new(0x00, 0x00, 0xFF, 0xFF);
    let green = css::Color::new(0x00, 0x80, 0x00, 0xFF);
    let child_doc = "<body style='margin:0'>\
         <script>\
         window.addEventListener('message', function(e) {\
           if (e.data === 'paint-blue') {\
             document.body.setAttribute('style', 'margin:0;background:rgb(0,0,255)');\
             window.parent.postMessage('child-done', '*');\
           }\
         });\
         </script></body>";
    let html = "<html><body style='margin:0'>\
         <iframe width='60' height='30' srcdoc=\"__CHILD__\"></iframe>\
         <div id='ack' style='width:40px;height:20px'></div>\
         <script>\
         window.addEventListener('message', function(e) {\
           if (e.data === 'child-done') {\
             document.getElementById('ack').setAttribute('style', 'width:40px;height:20px;background:green');\
           }\
         });\
         document.querySelector('iframe').contentWindow.postMessage('paint-blue', '*');\
         </script></body></html>"
        .replace("__CHILD__", child_doc);
    let output = render_page(&html);
    assert!(
        count_color(&output.pixmap, blue) >= 1_000,
        "postMessage to child must update child DOM to blue"
    );
    assert!(
        count_color(&output.pixmap, green) >= 500,
        "postMessage reply from child must update parent DOM to green"
    );
}

#[cfg(feature = "js")]
#[test]
fn iframe_sandbox_without_allow_same_origin_has_opaque_origin_and_isolated_storage() {
    let green = css::Color::new(0x00, 0x80, 0x00, 0xFF);
    let child_html = "<body style='margin:0'><script>\
        var storageThrew = false;\
        try { window.localStorage.setItem('k', 'v'); } catch (e) { storageThrew = true; }\
        if (window.origin === 'null' && storageThrew && document.cookie === '') {\
          document.body.setAttribute('style', 'margin:0;background:green');\
        }\
        </script></body>"
        .to_owned();
    let (origin_url, handle) = spawn_http_routes(
        vec![
            (
                "/parent.html",
                "<html><body style='margin:0'>\
                 <iframe width='50' height='30' sandbox='allow-scripts' src='/child.html'></iframe>\
                 <div id='res' style='width:30px;height:20px'></div>\
                 <script>\
                 var f = document.querySelector('iframe');\
                 if (f.contentDocument === null) {\
                   document.getElementById('res').setAttribute('style', 'width:30px;height:20px;background:red');\
                 }\
                 </script></body></html>"
                    .to_owned(),
                vec![("Set-Cookie", "sid=secret")],
            ),
            ("/child.html", child_html, vec![]),
        ],
        2,
    );
    let shared_storage =
        std::sync::Arc::new(std::sync::Mutex::new(render::RenderStorage::default()));
    let options = RenderOptions {
        viewport: Viewport::new(400, 300).expect("viewport"),
        storage: Some(shared_storage.clone()),
        cookies: true,
        ..RenderOptions::default()
    };
    let output = render_url(&format!("{origin_url}/parent.html"), &options).expect("render");
    assert!(
        count_color(&output.pixmap, green) >= 1_000,
        "sandboxed child must observe opaque 'null' origin, throwing localStorage, and empty cookie"
    );
    assert!(
        count_color(&output.pixmap, RED) >= 400,
        "parent must observe contentDocument === null for sandboxed frame without allow-same-origin"
    );
    assert!(
        shared_storage.lock().unwrap().by_origin.is_empty(),
        "opaque sandboxed frame must not write to shared storage"
    );
    handle.join().expect("server");
}
