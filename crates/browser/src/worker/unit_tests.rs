use super::*;
use std::time::Duration;

fn fake_frame(generation: u64) -> RenderFrame {
    RenderFrame {
        generation,
        width: 2,
        height: 2,
        pixels: vec![0; 4],
        png_bytes: vec![0x89, b'P', b'N', b'G'],
        content_height: 2,
        capped: false,
        links: Vec::new(),
        images: Vec::new(),
        title: None,
        document_url: "https://example.com/".to_string(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        forms: Vec::new(),
        document: Arc::new(megi_webview::core::Document::new()),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
        location_actions: Vec::new(),
        history_actions: Vec::new(),
    }
}

#[test]
fn frame_store_accepts_only_the_current_generation() {
    let mut store = FrameStore::new();
    assert_eq!(store.generation(), 0);
    let first = store.begin_request("data:,x", 800, 600);
    assert_eq!(first.generation, 1);
    assert!(store.pending());
    assert!(store.accept(WorkerMessage::Frame(fake_frame(1))));
    assert!(!store.pending());
    assert_eq!(store.frame().map(|frame| frame.generation), Some(1));

    let second = store.begin_request("data:,y", 640, 480);
    assert_eq!(second.generation, 2);
    // A stale frame (generation 1) must be ignored.
    assert!(!store.accept(WorkerMessage::Frame(fake_frame(1))));
    assert!(store.pending(), "stale result must not clear pending");
    assert!(store.accept(WorkerMessage::Frame(fake_frame(2))));
    assert_eq!(store.frame().map(|frame| frame.generation), Some(2));
}

#[test]
fn frame_store_records_errors_for_the_current_generation() {
    let mut store = FrameStore::new();
    let request = store.begin_request("data:,x", 800, 600);
    assert!(!store.accept(WorkerMessage::Failed {
        generation: request.generation - 1,
        message: "stale".to_string(),
    }));
    assert!(store.error().is_none());

    assert!(store.accept(WorkerMessage::Failed {
        generation: request.generation,
        message: "boom".to_string(),
    }));
    assert_eq!(store.error(), Some("boom"));
    assert_eq!(store.take_error(), Some("boom".to_string()));
    assert!(store.error().is_none());
    assert!(store.frame().is_none());
}

#[test]
fn debounce_coalesces_and_fires_once() {
    let start = Instant::now();
    let mut debounce = Debounce::new(Duration::from_millis(100));
    assert!(!debounce.is_due(start));
    let first = debounce.schedule(start);
    let second = debounce.schedule(start + Duration::from_millis(50));
    assert_eq!(second, first + Duration::from_millis(50));
    assert!(!debounce.take_due(start + Duration::from_millis(120)));
    assert!(debounce.take_due(second));
    assert!(!debounce.take_due(second), "fires only once");
    assert!(debounce.deadline().is_none());
    debounce.schedule(start);
    debounce.clear();
    assert!(!debounce.is_due(start + Duration::from_secs(1)));
}

#[test]
fn history_event_request_carries_popstate_and_hashchange() {
    // #1503: a same-document back/forward must render with host events so SPA listeners
    // fire. Verify the request builder stamps the events and keeps scripting enabled.
    let mut store = FrameStore::new();
    let request = store.begin_history_event_request(
        "data:text/html,<p>x</p>",
        320,
        200,
        Some("https://example.com/page#old"),
        &["hashchange", "popstate"],
    );
    assert_eq!(
        request.host_events,
        vec!["hashchange".to_owned(), "popstate".to_owned()]
    );
    assert!(request.scripting, "listeners must run on the snapshot");
    assert_eq!(
        request.base_override.as_deref(),
        Some("https://example.com/page#old")
    );
}

#[test]
fn view_options_applies_host_events() {
    // #1503: `view_options` must forward the request's events into RenderOptions so the
    // render pipeline dispatches them after `load`.
    let request = RenderRequest {
        generation: 1,
        input: "data:text/html,x".into(),
        css_width: 100,
        css_height: 100,
        raster_scale: 1.0,
        base_override: None,
        scripting: true,
        find_query: None,
        host_events: vec!["popstate".to_owned()],
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let viewport = megi_webview::render::Viewport::new(100, 100).expect("viewport");
    let options = view_options(
        &request,
        viewport,
        &megi_webview::render::RenderOptions::default(),
        false,
    );
    assert_eq!(options.host_events, vec!["popstate".to_owned()]);
}

#[test]
fn ui_event_request_and_view_options_propagate_host_dom_events() {
    let mut store = FrameStore::new();
    let events = vec![megi_webview::render::HostDomEvent {
        event_type: "input".to_owned(),
        target: megi_webview::render::HostEventTarget::FormControl(3),
        bubbles: true,
        cancelable: false,
        kind: megi_webview::render::HostUiEventKind::Input {
            data: Some("x".to_owned()),
            input_type: "insertText".to_owned(),
        },
    }];
    let request = store.begin_ui_event_request(
        "data:text/html,<input>",
        320,
        200,
        Some("https://example.com/form"),
        events.clone(),
    );
    assert!(request.scripting);
    assert_eq!(request.host_dom_events, events);
    let viewport = megi_webview::render::Viewport::new(320, 200).expect("viewport");
    let options = view_options(
        &request,
        viewport,
        &megi_webview::render::RenderOptions::default(),
        false,
    );
    assert_eq!(options.host_dom_events, events);
}

#[test]
fn worker_round_trips_requests_and_messages() {
    let (sender, receiver, handle) = spawn_worker(
        |request: &RenderRequest| Ok(fake_frame(request.generation)),
        || {},
    );
    let mut store = FrameStore::new();
    let request = store.begin_request("data:,x", 320, 200);
    sender.send(request).expect("worker request channel");
    let message = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("message");
    assert_eq!(message.generation(), 1);
    assert!(store.accept(message));
    drop(sender);
    handle.join().expect("worker exits when senders drop");
}

#[test]
fn worker_reports_render_errors_with_the_generation() {
    let (sender, receiver, handle) = spawn_worker(
        |request: &RenderRequest| Err(format!("fail {}", request.generation)),
        || {},
    );
    let mut store = FrameStore::new();
    sender
        .send(store.begin_request("data:,x", 100, 100))
        .expect("send");
    let message = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("message");
    assert!(matches!(
        &message,
        WorkerMessage::Failed { generation: 1, message } if message == "fail 1"
    ));
    assert!(store.accept(message));
    assert_eq!(store.error(), Some("fail 1"));
    drop(sender);
    handle.join().expect("join");
}

#[test]
fn render_document_uses_overflow_extent_for_scroll_height() {
    let html = "data:text/html,\
        <html><body style=\"margin:0;height:120px;overflow:visible\">\
        <div style=\"height:900px;width:50px\"></div>\
        </body></html>";
    let request = RenderRequest {
        generation: 1,
        input: html.to_string(),
        css_width: 320,
        css_height: 200,
        raster_scale: 1.0,
        base_override: None,
        scripting: false,
        find_query: None,
        host_events: Vec::new(),
        host_dom_events: Vec::new(),
        form_post: None,
    };
    let frame = render_document(&request).expect("render");
    // Before the fix the probe's root margin box (≈body height) reported a
    // scroll height equal to the viewport, so `max_offset()` stayed 0 and
    // wheel/keyboard scrolling did nothing. The scrollable overflow extent
    // must now include the 900 px child.
    assert!(
        frame.content_height > request.css_height,
        "content_height={} must exceed the viewport height={} when children overflow",
        frame.content_height,
        request.css_height
    );
    assert!(
        frame.content_height >= 800,
        "content_height={} should include the overflowing child extent",
        frame.content_height
    );
    assert_eq!(
        frame.content_height, frame.height,
        "the full-page render must cover the scrollable extent"
    );
}
