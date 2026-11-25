//! Worker coordination: render requests, generation-based message draining,
//! navigation-triggered renders and debounced resize scheduling.

use super::*;

/// Render options for one window render. The real winit device scale must reach the
/// renderer so page scripts (`window.devicePixelRatio`) and responsive image selection
/// see the same DPR as the window, instead of the pipeline's hardcoded 1.0.
pub(super) fn render_options_for_scale(
    base: &megi_webview::render::RenderOptions,
    scale_factor: f64,
) -> megi_webview::render::RenderOptions {
    let mut options = base.clone();
    options.device_pixel_ratio = crate::scale::sanitize_scale(scale_factor) as f32;
    options
}

impl WindowApp {
    /// Builds a render worker using this window's cloned options and event proxy.
    fn new_worker(&self) -> Worker {
        let render_options = render_options_for_scale(&self.render_options, self.scale_factor);
        let proxy = self.proxy.clone();
        spawn_worker(
            move |request: &RenderRequest| {
                crate::worker::render_document_with_options(request, &render_options)
            },
            move || {
                let _ = proxy.send_event(UserEvent::WorkerReady);
            },
        )
    }

    /// Queues a render to the per-site process pool (ADR-004 phase 3). Responses arrive
    /// asynchronously through the pool's reader threads and are drained like worker messages.
    fn send_process_render(&mut self, request: RenderRequest) -> bool {
        let wire = crate::ipc::IpcRenderRequest {
            generation: request.generation,
            input: request.input.clone(),
            css_width: request.css_width,
            css_height: request.css_height,
            base_override: request.base_override.clone(),
            scripting: request.scripting,
            find_query: request.find_query.clone(),
            options: crate::ipc::IpcRenderOptions::from_render_options(&render_options_for_scale(
                &self.render_options,
                self.scale_factor,
            )),
        };
        let pool = self.process_pool.get_or_insert_with(|| {
            crate::ipc::ProcessRenderPool::new(crate::ipc::MAX_PROCESS_RENDERERS)
        });
        match pool.send(wire) {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(format!("renderer process failed: {error}"));
                self.update_title();
                false
            }
        }
    }

    /// Routes a render request to the worker for its effective site (ADR-004 phase 2),
    /// spawning a new one on first use. Returns `false` on a channel failure.
    pub(super) fn send_render(&mut self, request: RenderRequest) -> bool {
        if self.use_process_render {
            return self.send_process_render(request);
        }
        let site = crate::worker::request_site(&request);
        let needs_new = self
            .render_pool
            .as_ref()
            .is_none_or(|pool| !pool.has_site(&site));
        let worker = if needs_new {
            Some(self.new_worker())
        } else {
            None
        };
        let pool = self
            .render_pool
            .get_or_insert_with(|| RendererPool::new(MAX_SITE_RENDERERS));
        if pool.send_prebuilt(request, worker).is_err() {
            self.error = Some("render worker stopped unexpectedly".to_string());
            self.update_title();
            return false;
        }
        true
    }

    /// Enqueues a full-page render for the current viewport.
    pub(super) fn request_render(&mut self) {
        let render_input = if crate::reader::is_reader_url(&self.input) {
            self.reader_html.clone().unwrap_or_else(|| {
                crate::pages::data_url(&crate::pages::reader_page("Reader", &[], "about:reader"))
            })
        } else if is_history_url(&self.input) {
            let html = crate::history::history_page_html(&self.tabs.active().history_log.recent());
            format!(
                "data:text/html;base64,{}",
                megi_webview::net::base64_encode(html.as_bytes())
            )
        } else if crate::pages::is_settings_url(&self.input) {
            // Like history, the generated page is rebuilt from live shell state here;
            // headless `prepare_input` keeps its deterministic all-off default.
            let html = crate::pages::settings_page_html_with_state(
                crate::i18n::Locale::current(),
                settings::settings_state(&self.chrome_mode, self.rail_open),
            );
            format!(
                "data:text/html;base64,{}",
                megi_webview::net::base64_encode(html.as_bytes())
            )
        } else {
            self.input.clone()
        };
        let page_height = self.page_viewport_height();
        let page_width = self.page_viewport_width();
        let request = if self.find.query().is_empty() {
            self.store
                .begin_request(&render_input, page_width, page_height)
        } else {
            self.store
                .begin_find_request(&render_input, page_width, page_height, self.find.query())
        };
        self.render_pending = true;
        self.send_render(request);
    }

    /// Clears all per-page state (scroll, stale links/forms/title/find, frame counters)    /// when navigating to a new page. Shared by `start_navigation` and form POSTs (#1824)
    /// so both behave as a real navigation instead of an in-place frame replacement.
    pub(super) fn clear_page_state(&mut self) {
        self.reader_html = None;
        self.error = None;
        self.content_capped = false;
        self.links.clear();
        self.refresh.clear();
        self.frame_scripts_executed = 0;
        self.frame_timers_pending = 0;
        self.frame_warnings = 0;
        self.frame_script_errors = 0;
        self.forms.clear();
        self.focus.blur();
        self.editor = None;
        self.document = None;
        self.live_forms.reset();
        self.doc_title = None;
        self.close_find();
        self.dropdown = None;
        self.overflow_menu = None;
        self.context_menu = None;
        self.history_menu = None;
        self.hover_url = None;
        self.address.cancel();
        self.scroll = ScrollState::new(1, self.page_viewport_height());
        self.update_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Starts rendering `url` and resets per-page state (scroll, stale links/title).
    pub(super) fn start_navigation(&mut self, url: &megi_webview::net::Url) {
        self.session_autosave.mark_dirty(Instant::now());
        self.input = url.as_str();
        if let Some((stored_zoom, zoom_changed)) =
            zoom::zoom_after_navigation(&self.site_zooms, &url.as_str(), self.zoom)
        {
            self.zoom = stored_zoom;
            self.tabs.active_mut().zoom = stored_zoom;
            if zoom_changed {
                // #1721: a persisted site zoom that differs from the current one must also
                // update the CSS viewport metrics — otherwise the frame is laid out at the
                // old zoom and page_coordinates() divides by a stale scale until the next
                // resize.
                let (width, height) = self
                    .window
                    .as_ref()
                    .map(|window| {
                        let size = window.inner_size();
                        (size.width, size.height)
                    })
                    .unwrap_or((self.requested_width, self.requested_height));
                self.metrics = window_metrics_zoomed(width, height, self.scale_factor, self.zoom);
            }
        }
        self.clear_page_state();
        self.request_render();
    }

    /// Commits a navigation (link click): pushes a history entry unless it is the same URL.
    pub(super) fn navigate_to(&mut self, url: megi_webview::net::Url) {
        if self.nav.navigate_to(url) == NavigationOutcome::Pushed
            && let Some(current) = self.nav.current().cloned()
        {
            self.start_navigation(&current);
        }
    }

    /// Goes back one history entry (no-op at the beginning of the history).
    pub(super) fn go_back(&mut self) {
        let previous = self.nav.current().cloned();
        if let Some(url) = self.nav.back().cloned() {
            self.apply_history_navigation(&url, previous.as_ref());
        }
    }

    /// Goes forward one history entry (no-op at the end of the history).
    pub(super) fn go_forward(&mut self) {
        let previous = self.nav.current().cloned();
        if let Some(url) = self.nav.forward().cloned() {
            self.apply_history_navigation(&url, previous.as_ref());
        }
    }

    /// Applies a back/forward navigation.
    ///
    /// When the target differs from the current document only by its fragment, the page is not
    /// reloaded: the current DOM snapshot is re-rendered with `hashchange` and `popstate`
    /// dispatched on `window` so SPA routers can react (#1503). Any other navigation falls back
    /// to a full render.
    /// Applies `location.*` / History API requests queued by page scripts in the
    /// last render (#1828). Same-document state changes only rewrite the URL;
    /// actual navigations reuse the existing link/back/forward paths.
    fn apply_script_navigation(
        &mut self,
        location_actions: Vec<megi_webview::render::LocationAction>,
        history_actions: Vec<megi_webview::render::HistoryAction>,
    ) {
        use megi_webview::render::{HistoryAction, LocationAction};
        for action in location_actions {
            let is_replace = matches!(action, LocationAction::Replace(_));
            match action {
                LocationAction::Assign(url) | LocationAction::Replace(url) => {
                    if let Ok(target) = megi_webview::net::Url::parse(&url) {
                        if is_replace && self.nav.replace_current(target.clone()) {
                            self.input = target.as_str();
                        } else {
                            self.navigate_to(target);
                        }
                    }
                }
                LocationAction::Reload => self.reload(),
            }
        }
        for action in history_actions {
            match action {
                HistoryAction::Back => self.go_back(),
                HistoryAction::Forward => self.go_forward(),
                HistoryAction::Go(0) => self.reload(),
                HistoryAction::Go(delta) => {
                    for _ in 0..delta.abs() {
                        if delta < 0 {
                            self.go_back();
                        } else {
                            self.go_forward();
                        }
                    }
                }
                // pushState/replaceState only change the URL/state for a
                // same-document navigation; the shell has no state store yet.
                HistoryAction::Push { url: Some(url), .. } => {
                    if let Ok(target) = megi_webview::net::Url::parse(&url) {
                        self.navigate_to(target);
                    }
                }
                HistoryAction::Replace { url: Some(url), .. } => {
                    if let Ok(target) = megi_webview::net::Url::parse(&url)
                        && self.nav.replace_current(target.clone())
                    {
                        self.input = target.as_str();
                    }
                }
                HistoryAction::Push { .. } | HistoryAction::Replace { .. } => {}
            }
        }
    }

    pub(super) fn apply_history_navigation(
        &mut self,
        url: &megi_webview::net::Url,
        previous: Option<&megi_webview::net::Url>,
    ) {
        let same_document = previous.is_some_and(|current| current.cache_key() == url.cache_key());
        let fragment_changed = previous.and_then(|current| current.fragment()) != url.fragment();
        if same_document
            && fragment_changed
            && self.document.is_some()
            && self.render_pool.is_some()
        {
            let snapshot =
                crate::forms::snapshot_data_url(self.document.as_ref().expect("document present"));
            // Resolve relative resources against the target URL (same document as `previous`).
            let base = Some(url.as_str());
            let events: Vec<&str> = if url.fragment().is_some() {
                vec!["hashchange", "popstate"]
            } else {
                vec!["popstate"]
            };
            let request = self.store.begin_history_event_request(
                &snapshot,
                self.page_viewport_width(),
                self.metrics.css_height,
                base.as_deref(),
                &events,
            );
            self.send_render(request);
            self.input = url.as_str();
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }
        self.start_navigation(url);
    }

    /// Reloads the current page (keeps scroll position and history).
    pub(super) fn reload(&mut self) {
        if self.nav.reload_target().is_some() {
            self.request_render();
        }
    }

    /// Debounced resize/scale change: schedule a render after the quiet period.
    pub(super) fn schedule_resize(&mut self, event_loop: &ActiveEventLoop, metrics: WindowMetrics) {
        self.pending_metrics = Some(metrics);
        let deadline = self.debounce.schedule(Instant::now());
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }

    /// Applies all pending worker messages; returns whether the view changed.
    pub(super) fn drain_worker_messages(&mut self) -> bool {
        let mut applied = false;
        let mut messages = Vec::new();
        if let Some(pool) = &mut self.render_pool {
            messages.extend(pool.drain_messages());
        }
        while let Some(message) = self.process_messages.pop_front() {
            messages.push(message);
        }
        if let Some(pool) = &mut self.process_pool {
            for (pending_gen, response) in pool.drain_tagged() {
                match response {
                    Ok(response) => {
                        let frame = crate::ipc::ProcessFrame::from_response(response);
                        match frame.to_render_frame(0) {
                            Ok(rendered) => {
                                messages.push(crate::worker::WorkerMessage::Frame(rendered))
                            }
                            Err(error) => messages.push(crate::worker::WorkerMessage::Failed {
                                generation: frame.response.generation,
                                message: error.to_string(),
                            }),
                        }
                    }
                    Err(error) => {
                        if let Some(generation) = pending_gen {
                            messages.push(crate::worker::WorkerMessage::Failed {
                                generation,
                                message: error.to_string(),
                            });
                        } else {
                            self.error = Some(error.to_string());
                        }
                    }
                }
            }
        }
        let mut sync_find = false;
        let mut rebuild_focus = false;
        for message in messages {
            if !self.store.accept(message) {
                continue; // stale generation
            }
            applied = true;
            match self.store.frame() {
                Some(frame) => {
                    rebuild_focus = true;
                    self.error = None;
                    self.render_pending = false;
                    self.content_capped = frame.capped;
                    self.links = frame.links.clone();
                    self.doc_title = frame.title.clone();
                    if !frame.document_url.starts_with("data:") {
                        let title = frame
                            .title
                            .clone()
                            .unwrap_or_else(|| frame.document_url.clone());
                        let logical = self.recent_history.record(&title, &frame.document_url);
                        self.tabs.active_mut().history_log.record(
                            &title,
                            &frame.document_url,
                            logical,
                        );
                    }
                    self.frame_scripts_executed = frame.scripts_executed;
                    self.frame_timers_pending = frame.script_timers_pending;
                    self.frame_warnings = frame.warnings;
                    self.frame_script_errors = frame.script_errors;
                    self.forms = frame.forms.clone();
                    self.find_rects = frame.find_rects.clone();
                    if self.find_editing && !self.find.query().is_empty() {
                        self.find.set_matches(frame.find_matches.clone());
                        sync_find = true;
                    }
                    // A patched-DOM render has a data: document URL: keep the real page URL
                    // in history/`input` (only store redirects for normal renders).
                    let patched_render = frame.document_url.starts_with("data:");
                    if !patched_render {
                        self.editor = None;
                        self.release_button = None;
                    }
                    self.scroll.set_content_height(frame.content_height);
                    // Auto-refresh only when the page ran scripts and left timers pending;
                    // one-shot: the next frame re-arms it or clears it (no loops otherwise).
                    if self
                        .refresh
                        .should_refresh(frame.scripts_executed, frame.script_timers_pending)
                    {
                        self.refresh.schedule(Instant::now());
                    } else {
                        self.refresh.clear();
                    }
                    // Server redirects: the final URL replaces the requested history entry.
                    if !patched_render
                        && let Ok(final_url) = megi_webview::net::Url::parse(&frame.document_url)
                        && self.nav.current() != Some(&final_url)
                        && self.nav.replace_current(final_url.clone())
                    {
                        self.input = final_url.as_str();
                    }
                    // Take unique ownership of the frame's Document (`strong_count == 1`) so live
                    // form edits mutate in-place via `Arc::make_mut` without cloning the DOM (#2199).
                    // #1828: consume navigation requests emitted by page scripts
                    // during this render (location.*, history.*).
                    let script_location_actions = frame.location_actions.clone();
                    let script_history_actions = frame.history_actions.clone();
                    self.document = self.store.take_frame_document();
                    self.live_forms.on_frame_applied();
                    self.apply_script_navigation(script_location_actions, script_history_actions);
                }
                None => {
                    self.render_pending = false;
                    self.error = self.store.take_error();
                    // A failed render must not keep auto-refreshing (error loop).
                    self.refresh.clear();
                    if let Some(error) = &self.error {
                        eprintln!("megi: {error}");
                    }
                }
            }
            if sync_find {
                sync_find = false;
                self.scroll_to_current_find();
            }
        }
        if applied {
            // Rebuild the keyboard focus list for the new DOM, keeping focus when the same
            // target survived the render. Done here, after the `frame` borrow ended (#1517).
            if rebuild_focus {
                self.rebuild_focus_targets();
            }
            self.update_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            // Release a pressed button visual state once its frame is on screen.
            if let Some(form_id) = self.release_button.take() {
                self.apply_form_patch(Some(form_id), None, None, false);
            }
        }
        applied
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn window_render_options_carry_device_pixel_ratio() {
        let options =
            render_options_for_scale(&megi_webview::render::RenderOptions::default(), 1.25);
        assert_eq!(options.device_pixel_ratio, 1.25);
    }
}
