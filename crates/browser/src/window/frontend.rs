//! Frontend setup: construction of [`WindowApp`] and winit/softbuffer/window wiring.

use super::*;

impl WindowApp {
    pub(super) fn new(
        input: String,
        title: String,
        width: u32,
        height: u32,
        proxy: EventLoopProxy<UserEvent>,
        render_options: megi_webview::render::RenderOptions,
        view: WindowViewOptions,
    ) -> Self {
        let mut startup_notice = None;
        let mut recent = RecentHistory::new();
        let mut site_zooms: Vec<(String, f64)> = Vec::new();
        let tabs = if let Some(path) = &view.session_path {
            let loaded = crate::session::load(path);
            if let Some(warning) = &loaded.warning {
                eprintln!("megi: {warning}");
                startup_notice = Some(warning.clone());
            }
            match loaded.session {
                Some(session) => {
                    startup_notice = Some(format!("restored {} tab(s)", session.tabs.len()));
                    recent = crate::history::RecentHistory::from_entries(session.recent.clone());
                    site_zooms = session.site_zooms.clone();
                    let mut restored = session.restore();
                    if !input.is_empty() && input != crate::tabs::BLANK_TAB_URL {
                        if let Some(pos) = restored.tabs().iter().position(|tab| tab.url == input) {
                            restored.activate(pos);
                        } else if restored.active().url == crate::tabs::BLANK_TAB_URL {
                            if let Some(replacement) = crate::tabs::Tab::new(input.clone()) {
                                *restored.active_mut() = replacement;
                            }
                        } else {
                            restored.open_tab(input.clone());
                        }
                    }
                    restored
                }
                None => TabsState::from_urls(view.tabs.clone()),
            }
        } else if view.tabs.is_empty() {
            TabsState::new(input.clone())
        } else {
            TabsState::from_urls(view.tabs.clone())
        };
        let input = tabs.active().url.clone();
        let nav = tabs.active().navigation.clone();
        let zoom = zoom::sanitize_zoom(tabs.active().zoom);
        let restored_scroll = tabs.active().scroll;
        Self {
            input,
            title,
            requested_width: width,
            requested_height: height,
            window: None,
            context: None,
            surface: None,
            proxy,
            render_options,
            metrics: window_metrics_zoomed(width, height, 1.0, zoom),
            scale_factor: 1.0,
            scroll: restored_scroll,
            store: FrameStore::new(),
            render_pool: None,
            process_session: None,
            use_process_render: view.process_render,
            process_messages: std::collections::VecDeque::new(),
            debounce: Debounce::new(RENDER_DEBOUNCE),
            pending_metrics: None,
            error: None,
            content_capped: false,
            nav,
            links: Vec::new(),
            doc_title: None,
            cursor: None,
            modifiers: ModifiersState::default(),
            zoom,
            refresh: RefreshTimer::new(view.refresh),
            frame_scripts_executed: 0,
            frame_timers_pending: 0,
            frame_warnings: 0,
            frame_script_errors: 0,
            forms: Vec::new(),
            focus: FocusState::new(),
            focus_targets: Vec::new(),
            editor: None,
            ime: crate::forms::ImeState::new(),
            save_dialog: None,
            document: None,
            release_button: None,
            find: FindState::new(),
            find_editing: false,
            find_rects: Vec::new(),
            address: AddressBar::new(),
            dropdown: None,
            session_path: view.session_path.clone(),
            download_dir: view.download_dir.clone(),
            session_autosave: SessionAutosave::new(std::time::Duration::from_millis(600)),
            status: startup_notice,
            render_pending: false,
            overflow_menu: None,
            hover_url: None,
            context_menu: None,
            clipboard: String::new(),
            #[cfg(feature = "a11y")]
            accessibility_adapter: None,
            #[cfg(feature = "a11y")]
            accessibility_publisher: crate::a11y_backend::AccessibilityPublisher::new(),
            recent_history: recent,
            history_menu: None,
            download: crate::download::DownloadProgress::new(),
            download_manager: crate::download::DownloadManager::new(
                crate::download::DEFAULT_MAX_JOBS,
                crate::download::DEFAULT_MAX_ACTIVE,
                crate::download::DEFAULT_MAX_BYTES,
            ),
            multi_anchor: None,
            hover_form: None,
            site_zooms,
            reader_html: None,
            tabs,
        }
    }

    /// Creates the window, surface and worker thread on first resume.
    pub(super) fn ensure_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), ShellError> {
        if self.window.is_some() {
            return Ok(());
        }
        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(self.requested_width),
                f64::from(self.requested_height),
            ))
            // ADR-002: the AccessKit adapter must be created before the window is shown.
            .with_visible(cfg!(not(feature = "a11y")));
        let window =
            Arc::new(event_loop.create_window(attributes).map_err(|error| {
                ShellError::Window(format!("cannot create the window: {error}"))
            })?);
        let context = softbuffer::Context::new(Arc::clone(&window))
            .map_err(|error| ShellError::Window(format!("softbuffer context: {error}")))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window))
            .map_err(|error| ShellError::Window(format!("softbuffer surface: {error}")))?;

        self.scale_factor = window.scale_factor();
        let size = window.inner_size();
        self.metrics = window_metrics_zoomed(size.width, size.height, self.scale_factor, self.zoom);
        self.scroll = ScrollState::new(1, self.page_viewport_height());
        #[cfg(feature = "a11y")]
        {
            // Must happen before the window becomes visible, per accesskit_winit docs.
            let adapter = accesskit_winit::Adapter::with_event_loop_proxy(
                event_loop,
                &window,
                self.proxy.clone(),
            );
            self.accessibility_adapter = Some(adapter);
            window.set_visible(true);
        }
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);

        // ADR-004 phase 2: renderers are spawned lazily, one per effective site, by
        // `WindowApp::send_render`. No worker exists until the first request arrives.
        Ok(())
    }
}
