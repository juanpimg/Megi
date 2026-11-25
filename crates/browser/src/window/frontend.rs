//! Frontend setup: construction of [`WindowApp`] and winit/softbuffer/window wiring.

use super::shortcuts::ShellAction;
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
        // #1725: reader-mode page persisted with the session (window-global view).
        let mut reader_html: Option<String> = None;
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
                    reader_html = session.reader_html.clone();
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
            process_pool: None,
            use_process_render: view.process_render,
            process_messages: std::collections::VecDeque::new(),
            debounce: Debounce::new(RENDER_DEBOUNCE),
            find_render: crate::worker::FindRenderCoalescer::new(RENDER_DEBOUNCE),
            pending_metrics: None,
            error: None,
            content_capped: false,
            nav,
            links: Vec::new(),
            doc_title: None,
            cursor: None,
            chrome_mode: super::chrome_mode::ChromeMode::new(view.reduced_motion),
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
            live_forms: crate::forms::LiveFormPatcher::new(),
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
            download_panel: crate::window::downloads::DownloadPanelState::new(),
            download_manager: crate::download::DownloadManager::new(
                crate::download::DEFAULT_MAX_JOBS,
                crate::download::DEFAULT_MAX_ACTIVE,
                crate::download::DEFAULT_MAX_BYTES,
            ),
            multi_anchor: None,
            hover_form: None,
            site_zooms,
            reader_html,
            tabs,
            rail_open: false,
            rail_expanded: false,
            rail_hover: false,
            rail_labels: Vec::new(),
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
        // #1823: `WindowApp::new` restored the session's scroll with a synthetic
        // content height of `offset + 1` and a 1px viewport. Replacing it with a
        // fresh state here dropped the offset, so every restart opened at the top.
        // Keep the restored offset by rebuilding with a provisional content height
        // of `offset + viewport`; the first real frame's `set_content_height`
        // then clamps it against the actual document height.
        let restored_offset = self.scroll.offset();
        let viewport = self.page_viewport_height();
        self.scroll = ScrollState::new(restored_offset.saturating_add(viewport), viewport);
        self.scroll.scroll_to(restored_offset);
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

    /// Applies a migrated shell action and reports whether the key was consumed.
    pub(super) fn dispatch_shortcut(&mut self, action: Option<ShellAction>) -> bool {
        match action {
            Some(ShellAction::ToggleReader) => self.open_reader(),
            Some(ShellAction::ToggleHistory) => {
                if self.history_menu.is_some() {
                    self.history_menu = None;
                } else {
                    let rect = chrome::address_bar_rect(self.metrics.css_width as f32);
                    self.history_menu = Some(HistoryMenu::open(
                        self.recent_history.menu_entries(),
                        rect.x,
                        rect.y + rect.height,
                    ));
                }
                self.request_repaint();
            }
            Some(ShellAction::SaveFrame) => self.save_frame_png(),
            Some(ShellAction::SaveAs) => {
                // Standard "Save As" shortcut opens the visual dialog (#988).
                self.open_save_dialog(true);
            }
            Some(ShellAction::FocusAddressBar) => self.open_address_bar(),
            Some(ShellAction::OpenFind) => self.open_find(),
            Some(ShellAction::PrintDocumentInfo) => self.print_document_info(),
            Some(ShellAction::NewTab) => self.open_new_tab(),
            Some(ShellAction::CloseTab) => self.close_active_tab(),
            Some(ShellAction::CycleTab { next }) => self.cycle_tab(next),
            Some(ShellAction::ActivateTab(index)) => self.activate_tab(index),
            Some(ShellAction::ToggleRail) => self.toggle_rail(),
            Some(ShellAction::ToggleCompact) => self.toggle_compact(),
            Some(ShellAction::CycleWorkspace) => self.cycle_workspace(),
            Some(ShellAction::NewWorkspace) => self.new_workspace(),
            Some(ShellAction::CloseWorkspace) => self.close_workspace(),
            _ => {}
        }
        // Only migrated actions return early; all other bindings keep falling through
        // to the legacy handlers below, preserving their original behaviour.
        matches!(
            action,
            Some(
                ShellAction::ToggleReader
                    | ShellAction::ToggleHistory
                    | ShellAction::SaveFrame
                    | ShellAction::SaveAs
                    | ShellAction::FocusAddressBar
                    | ShellAction::OpenFind
                    | ShellAction::PrintDocumentInfo
                    | ShellAction::NewTab
                    | ShellAction::CloseTab
                    | ShellAction::CycleTab { .. }
                    | ShellAction::ActivateTab(_)
                    | ShellAction::ToggleRail
                    | ShellAction::ToggleCompact
                    | ShellAction::CycleWorkspace
                    | ShellAction::NewWorkspace
                    | ShellAction::CloseWorkspace
            )
        )
    }
}

/// Phase 2 shell state for the compact band and the transient rail peek.
///
/// Compact mode is opt-in and only changes the chrome band. A rail peek is a
/// transient overlay: it never changes the page inset or the viewport, and a
/// latched rail (`Ctrl+B`) is the only state that reserves horizontal space.
pub(super) mod chrome_mode {

    use crate::chrome;
    use std::time::{Duration, Instant};

    /// Geometry snapshot consumed by input handling and presentation.
    #[allow(dead_code)]
    #[derive(Clone, Copy, Debug)]
    pub(in crate::window) struct ChromeState {
        pub compact: bool,
        pub band_css: f32,
        pub inset_css: f32,
        pub rail_width_css: f32,
        pub labels_visible: bool,
    }

    #[derive(Clone, Debug)]
    pub(in crate::window) struct ChromeMode {
        compact: bool,
        rail_open: bool,
        peek_start: Option<Instant>,
        reduced_motion: bool,
    }

    impl ChromeMode {
        pub(in crate::window) fn new(reduced_motion: bool) -> Self {
            Self {
                compact: false,
                rail_open: false,
                peek_start: None,
                reduced_motion,
            }
        }

        pub(in crate::window) fn compact(&self) -> bool {
            self.compact
        }

        pub(in crate::window) fn reduced_motion(&self) -> bool {
            self.reduced_motion
        }

        /// Sets the value written by the settings page's compact-mode switch.
        pub(in crate::window) fn set_compact(&mut self, compact: bool) {
            self.compact = compact;
        }

        /// Sets the value written by the settings page's reduced-motion switch.
        pub(in crate::window) fn set_reduced_motion(&mut self, reduced_motion: bool) {
            self.reduced_motion = reduced_motion;
        }

        /// Aligns the peek geometry with the rail value written by the settings page.
        pub(in crate::window) fn set_rail_open(&mut self, open: bool) {
            if self.rail_open != open {
                self.rail_open = open;
                self.peek_start = None;
            }
        }

        fn band_css_height(&self) -> f32 {
            chrome::band_css_height(self.compact)
        }

        /// Space reserved for the rail. Peeking paints over the page, so the inset
        /// stays at the collapsed width while labels are merely visible.
        fn page_inset_css(&self, _now: Instant) -> f32 {
            if self.rail_open {
                chrome::RAIL_EXPANDED_WIDTH
            } else {
                chrome::RAIL_COLLAPSED_WIDTH
            }
        }

        fn rail_width_css(&self, now: Instant) -> f32 {
            chrome::peek_width_css(
                self.peek_start.is_some(),
                self.rail_open,
                self.elapsed_ms(now),
                self.reduced_motion,
            )
        }

        fn labels_visible(&self, now: Instant) -> bool {
            chrome::peek_labels_visible(
                self.peek_start.is_some(),
                self.rail_open,
                self.elapsed_ms(now),
                self.reduced_motion,
            )
        }

        /// Single narrow projection of the private geometry helpers.
        pub(in crate::window) fn state(&self, now: Instant) -> ChromeState {
            ChromeState {
                compact: self.compact(),
                band_css: self.band_css_height(),
                inset_css: self.page_inset_css(now),
                rail_width_css: self.rail_width_css(now),
                labels_visible: self.labels_visible(now),
            }
        }

        pub(in crate::window) fn toggle_compact(&mut self) {
            self.compact = !self.compact;
        }

        #[allow(dead_code)]
        pub(in crate::window) fn toggle_rail(&mut self) {
            self.rail_open = !self.rail_open;
            self.peek_start = None;
        }

        /// Returns `true` when the peek state changed and a repaint is due.
        pub(in crate::window) fn set_peek(&mut self, hovering: bool, now: Instant) -> bool {
            if self.rail_open {
                let changed = self.peek_start.is_some();
                self.peek_start = None;
                changed
            } else if hovering && self.peek_start.is_none() {
                self.peek_start = Some(now);
                true
            } else if !hovering && self.peek_start.is_some() {
                self.peek_start = None;
                true
            } else {
                false
            }
        }

        /// Deadline for the one final repaint that settles the peek animation.
        /// Reduced motion jumps straight to the final width and needs no timer.
        #[allow(dead_code)]
        pub(in crate::window) fn peek_deadline(&self) -> Option<Instant> {
            if self.reduced_motion || self.rail_open {
                return None;
            }
            self.peek_start
                .map(|start| start + Duration::from_millis(u64::from(chrome::PEEK_ANIMATION_MS)))
        }

        fn elapsed_ms(&self, now: Instant) -> u32 {
            self.peek_start.map_or(0, |start| {
                let millis = now.saturating_duration_since(start).as_millis();
                millis.min(u128::from(u32::MAX)) as u32
            })
        }
    }
}
