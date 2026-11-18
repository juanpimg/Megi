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
fn webview_navigation_history_is_platform_independent() {
    let mut view = WebView::new(WebViewOptions::new(64, 64).expect("options")).expect("viewport");
    assert!(!view.can_back());
    view.load("data:text/html,one").expect("first page");
    view.load("data:text/html,two").expect("second page");
    assert!(view.can_back());
    assert!(view.back().is_some());
    assert!(view.can_forward());
}
