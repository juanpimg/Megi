use super::*;

/// Debounce timer for coalescing resize/scale renders.
#[derive(Clone, Debug)]
pub struct Debounce {
    delay: Duration,
    deadline: Option<Instant>,
}

impl Debounce {
    /// Creates a debounce with the given delay.
    pub const fn new(delay: Duration) -> Self {
        Self {
            delay,
            deadline: None,
        }
    }

    /// (Re)starts the timer from `now`; returns the new deadline.
    pub fn schedule(&mut self, now: Instant) -> Instant {
        let deadline = now + self.delay;
        self.deadline = Some(deadline);
        deadline
    }

    /// Cancels any pending fire.
    pub fn clear(&mut self) {
        self.deadline = None;
    }

    /// Deadline of the pending timer, if any.
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Whether the timer is due at `now`.
    pub fn is_due(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    /// Fires the timer when due, clearing it; returns whether it fired.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.is_due(now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

/// Extracts a human-readable message from a caught panic payload.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Spawns the render worker.
///
/// * `render` is injected: production uses [`render_document`]; tests use a
///   fake renderer.
/// * `notify` is called after every message is queued (the window passes a
///   winit event-loop proxy so the event loop wakes up without polling).
///
/// The returned [`Sender`]/[`Receiver`] are the request/result channels; the
/// worker exits when all senders are dropped.
pub fn spawn_worker<R, N>(
    mut render: R,
    notify: N,
) -> (
    Sender<RenderRequest>,
    Receiver<WorkerMessage>,
    JoinHandle<()>,
)
where
    R: FnMut(&RenderRequest) -> Result<RenderFrame, String> + Send + 'static,
    N: Fn() + Send + 'static,
{
    let (request_sender, request_receiver) = std::sync::mpsc::channel::<RenderRequest>();
    let (message_sender, message_receiver) = std::sync::mpsc::channel::<WorkerMessage>();
    let handle = thread::spawn(move || {
        // Exits when all senders are dropped (event loop closed).
        while let Ok(first) = request_receiver.recv() {
            // Coalesce: skip every request that arrived while we were idle.
            let mut latest = first;
            loop {
                match request_receiver.try_recv() {
                    Ok(newer) => latest = newer,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            // A panic in layout/paint/render must not kill the only render worker:
            // convert it into a Failed message and keep serving later requests (#1468).
            let rendered =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(&latest)));
            let message = match rendered {
                Ok(Ok(frame)) => WorkerMessage::Frame(frame),
                Ok(Err(message)) => WorkerMessage::Failed {
                    generation: latest.generation,
                    message,
                },
                Err(panic) => {
                    let detail = panic_message(&panic);
                    eprintln!("megi: render worker caught a panic: {detail}");
                    WorkerMessage::Failed {
                        generation: latest.generation,
                        message: format!("render panicked: {detail}"),
                    }
                }
            };
            if message_sender.send(message).is_err() {
                break; // event loop is gone
            }
            notify();
        }
    });
    (request_sender, message_receiver, handle)
}

/// Renderer used by the window: renders the whole page at the CSS viewport
/// width and clamps its height to the render cap.
///
/// It renders a cheap viewport-height probe first to learn the document height
/// and only re-renders at full content height when the page is taller than the
/// window (typical pages: one render).
pub fn render_document(request: &RenderRequest) -> Result<RenderFrame, String> {
    render_document_with_options(request, &megi_webview::render::RenderOptions::default())
}

/// [`render_document`] with base [`megi_webview::render::RenderOptions`] (cookie jar, CORS origin,
/// scripting toggles, network caps) applied to every render of the window.
pub fn render_document_with_options(
    request: &RenderRequest,
    base_options: &megi_webview::render::RenderOptions,
) -> Result<RenderFrame, String> {
    // Height probe must not run page scripts: a tall page would otherwise execute
    // timers/fetches/storage writes twice (once for the probe, once at full height, #1471).
    let probe = render_view(request, request.css_height, base_options, true)?;
    let layout_height =
        scrollable_content_height(&probe.layout).max(probe.layout.dimensions.margin_box().height);
    let (content_height, capped) = scroll::content_height(layout_height);
    let target_height = content_height.clamp(1, scroll::MAX_CONTENT_HEIGHT);
    // Compare **CSS** heights: `probe.pixmap.height` is physical when raster_scale != 1.
    let output = if target_height > request.css_height {
        render_view(request, target_height, base_options, false)?
    } else if request.scripting {
        // Re-run at probe height with scripting enabled so scripts execute once.
        render_view(request, request.css_height, base_options, false)?
    } else {
        probe
    };
    let pixels = pixmap_to_0rgb(&output.pixmap);
    let links = collect_link_regions(&output);
    let images = crate::images::collect_image_regions(
        &output.document,
        &output.layout,
        Some(&output.document_url),
    );
    let title = document_title(&output.document);
    let document_url = output.document_url.as_str();
    let document_url =
        if document_url.starts_with("data:text/html") && !request.input.starts_with("data:") {
            request.input.clone()
        } else {
            document_url.to_owned()
        };
    let mut document = output.document;
    crate::forms::assign_form_ids(&mut document);
    let forms = crate::forms::collect_controls(&document, &output.layout);
    let find_query = request.find_query.as_deref().unwrap_or("");
    let (find_matches, find_rects) = if find_query.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            crate::find::search(&document, find_query),
            crate::find::highlight_rects(&output.layout, find_query),
        )
    };
    let scripts_executed = output.stats.scripts_executed;
    let script_timers_pending = output.stats.script_timers_pending;
    // Surface scripting/parser budget exhaustion through the same warning channel the
    // window title and headless CLI already consume (#1458).
    let mut warning_messages = output.stats.warnings.clone();
    if output.stats.script_limited {
        warning_messages.push("scripting budget exhausted".to_string());
    }
    let violations = output.stats.limit_violations.total();
    if violations > 0 {
        warning_messages.push(format!("{violations} parser/layout limit violation(s)"));
    }
    let warnings = warning_messages.len();
    let script_errors = output.stats.scripts_failed
        + output
            .stats
            .console
            .iter()
            .filter(|message| message.level == "error")
            .count();
    let png_bytes = output.png.clone();
    Ok(RenderFrame {
        generation: request.generation,
        width: output.pixmap.width,
        height: output.pixmap.height,
        pixels,
        png_bytes,
        content_height: target_height,
        capped,
        links,
        images,
        title,
        document_url: document_url.clone(),
        scripts_executed,
        script_timers_pending,
        warnings,
        script_errors,
        forms,
        document: Arc::new(document),
        find_matches,
        find_rects,
        location_actions: output.location_actions.clone(),
        history_actions: output.history_actions.clone(),
    })
}

/// Computes the scrollable overflow height of a laid-out document.
///
/// Walks the layout tree and returns the maximum bottom edge (`y + height`) of
/// every margin box in CSS pixels. Descendants of boxes whose computed
/// `overflow` is not `visible` are not traversed for page-scroll extent,
/// because `hidden`/`clip` clip their overflow and `scroll`/`auto` establish an
/// inner scroll container (issue #1021). The root margin box is still included.
pub(crate) fn scrollable_content_height(root: &LayoutBox) -> f32 {
    fn walk(node: &LayoutBox, max_bottom: &mut f32) {
        // `position: fixed` boxes are viewport-relative and do not participate in
        // document scrolling, so a fixed footer/header must not inflate the page
        // scroll height.
        if matches!(node.position, Position::Fixed) {
            return;
        }
        let margin = node.dimensions.margin_box();
        let bottom = margin.y + margin.height;
        if bottom.is_finite() {
            *max_bottom = max_bottom.max(bottom);
        }
        let clips_overflow = node
            .style()
            .is_some_and(|style| !matches!(style.overflow, Overflow::Visible));
        if clips_overflow {
            return;
        }
        for child in &node.children {
            walk(child, max_bottom);
        }
    }

    let mut max_bottom = 0.0f32;
    walk(root, &mut max_bottom);
    max_bottom.max(0.0)
}

/// Builds the `RenderOptions` for one pass. The height probe (`probe = true`) always
/// disables scripting so tall pages do not run scripts twice (#1471).
pub(crate) fn view_options(
    request: &RenderRequest,
    viewport: megi_webview::render::Viewport,
    base_options: &megi_webview::render::RenderOptions,
    probe: bool,
) -> megi_webview::render::RenderOptions {
    let mut options = base_options.clone();
    options.viewport = viewport;
    // The CSS viewport above is unchanged; the raster pass alone is scaled by
    // the device scale carried in the request. Hit regions stay in CSS px.
    options.raster_scale = request.raster_scale;
    if let Some(base) = &request.base_override {
        options.base_override = Some(base.clone());
    }
    // The height probe never executes page scripts (#1471); the final render honors
    // the caller's scripting toggle.
    if probe || !request.scripting {
        options.scripting = megi_webview::render::ScriptingMode::Disabled;
    }
    // Host-dispatched events run on `window` after `load`, before the final re-render (#1503).
    options.host_events = request.host_events.clone();
    options.host_dom_events = request.host_dom_events.clone();
    options.form_post = request.form_post.clone();
    options
}

/// Runs one `render` pass at `css_width x height`, applying `base_options`.
pub(crate) fn render_view(
    request: &RenderRequest,
    height: u32,
    base_options: &megi_webview::render::RenderOptions,
    probe: bool,
) -> Result<megi_webview::render::RenderOutput, String> {
    let viewport =
        megi_webview::render::Viewport::new(request.css_width, height).map_err(|error| {
            format!(
                "invalid render viewport {}x{height}: {error}",
                request.css_width
            )
        })?;
    let options = view_options(request, viewport, base_options, probe);
    let prepared = match crate::pages::prepare_input(&request.input) {
        Ok(prepared) => prepared,
        Err((code, message)) => {
            let page =
                crate::pages::data_url(&crate::pages::error_html(&request.input, &code, &message));
            return megi_webview::render::render_url(&page, &options)
                .map_err(|error| error.to_string());
        }
    };
    match megi_webview::render::render_url(&prepared, &options) {
        Ok(output) => Ok(output),
        Err(error) => {
            // Browser error page for DNS/HTTP/TLS/file failures instead of a blank window.
            let code = crate::pages::error_code(&error);
            let page = crate::pages::data_url(&crate::pages::error_html(
                &request.input,
                code,
                &error.to_string(),
            ));
            megi_webview::render::render_url(&page, &options).map_err(|_| error.to_string())
        }
    }
}

/// Coalesces find-in-page query changes into a single render (F7-FIND-IN-PAGE-PERF / #1830).
///
/// Find highlights are computed from the document during the render worker pass, so every
/// keystroke would otherwise trigger a full re-fetch + re-layout + re-encode cycle. A burst of
/// query changes schedules one render after the debounce window; clearing the query renders
/// immediately so stale highlights disappear without waiting.
#[derive(Clone, Debug)]
pub struct FindRenderCoalescer {
    debounce: Debounce,
    pending: bool,
}

impl FindRenderCoalescer {
    pub const fn new(delay: Duration) -> Self {
        Self {
            debounce: Debounce::new(delay),
            pending: false,
        }
    }

    /// Records a query change; re-arms the debounce without rendering yet.
    pub fn on_query_changed(&mut self, now: Instant) -> Instant {
        self.pending = true;
        self.debounce.schedule(now)
    }

    /// True when a scheduled render is due; consumes the pending flag.
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.pending && self.debounce.take_due(now) {
            self.pending = false;
            true
        } else {
            false
        }
    }

    /// Clearing the query must clear stale highlights immediately: cancels the debounced
    /// render and returns true once.
    pub fn take_due_immediate(&mut self) -> bool {
        self.debounce.clear();
        std::mem::take(&mut self.pending)
    }

    pub const fn deadline(&self) -> Option<Instant> {
        self.debounce.deadline()
    }
}

#[cfg(test)]
mod find_coalescer_tests {
    use super::*;

    #[test]
    fn find_query_changes_coalesce_into_one_render() {
        // #1830: typing 10 characters in Ctrl+F triggered 10 full re-renders. The
        // coalescer must schedule a single render for a burst of query changes.
        let mut coalescer = FindRenderCoalescer::new(Duration::from_millis(120));
        let start = Instant::now();
        for i in 0..10 {
            let now = start + Duration::from_millis(i * 20);
            coalescer.on_query_changed(now);
            assert!(!coalescer.take_due(now), "not due during the burst");
        }
        assert!(
            coalescer.take_due(start + Duration::from_millis(300)),
            "one render is due after the debounce elapses"
        );
        assert!(
            !coalescer.take_due(start + Duration::from_millis(400)),
            "a second poll must not schedule another render"
        );
    }

    #[test]
    fn find_query_cleared_renders_immediately() {
        let mut coalescer = FindRenderCoalescer::new(Duration::from_millis(120));
        let start = Instant::now();
        coalescer.on_query_changed(start);
        assert!(
            coalescer.take_due_immediate(),
            "clearing the query must clear highlights now"
        );
        assert!(
            !coalescer.take_due(start),
            "the pending render was cancelled"
        );
    }
}
