//! A host can embed Megi without any windowing dependency.

use megi_webview::{WebView, WebViewOptions};

#[test]
fn webview_renders_a_data_url_without_a_window() {
    let mut view = WebView::new(WebViewOptions::new(320, 200).expect("options")).expect("viewport");
    let frame = view
        .load("data:text/html,<h1>Hello Megi</h1>")
        .expect("render data URL");
    assert_eq!(frame.pixmap.width, 320);
    assert_eq!(frame.pixmap.height, 200);
    assert_eq!(&frame.png[1..4], b"PNG");
    assert!(view.frame().is_some());
}

#[test]
fn webview_failed_back_keeps_the_current_history_entry() {
    // #1720: back() moved the index before rendering. A render failure left
    // current_url() pointing at an entry that was never displayed.
    let path = std::env::temp_dir().join(format!(
        "megi-webview-history-1720-{}.html",
        std::process::id()
    ));
    std::fs::write(&path, "<h1>one</h1>").expect("write fixture");
    let first_url = format!("file://{}", path.display());
    let mut view = WebView::new(WebViewOptions::new(64, 64).expect("options")).expect("viewport");
    view.load(&first_url).expect("first page");
    view.load("data:text/html,two").expect("second page");
    std::fs::remove_file(&path).expect("remove fixture");

    assert!(view.back().is_none(), "back to a missing file must fail");
    assert_eq!(
        view.current_url(),
        Some("data:text/html,two"),
        "a failed back() must not move the history cursor"
    );
}

#[test]
fn webview_navigation_history_is_platform_independent() {
    let mut view = WebView::new(WebViewOptions::new(64, 64).expect("options")).expect("viewport");
    assert!(!view.can_back());
    view.load("data:text/html,one").expect("first page");
    view.load("data:text/html,two").expect("second page");
    assert!(view.can_back());
    assert!(view.back().is_some());
    assert!(view.can_forward());
}

#[test]
fn webview_event_queue_is_bounded() {
    let mut view = WebView::new(WebViewOptions::new(320, 240).expect("viewport")).expect("webview");
    for _ in 0..(megi_webview::MAX_QUEUED_EVENTS + 500) {
        view.enqueue_event(megi_webview::WebEvent::Wheel { dx: 0.0, dy: 1.0 });
    }
    let drained = view.drain_events();
    assert_eq!(drained.len(), megi_webview::MAX_QUEUED_EVENTS);
}
