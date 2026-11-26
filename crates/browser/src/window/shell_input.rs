//! Unified shell input: view construction, hit-testing, focus traversal and
//! action dispatch shared by pointer and keyboard handling.

use super::*;

impl WindowApp {
    /// Builds the same shell view the painter uses, so hit-testing, focus order
    /// and the hover/focus tint can never disagree about the painted controls.
    fn with_shell_view<R>(&self, f: impl FnOnce(&chrome::ShellView<'_>) -> R) -> R {
        let labels = super::presentation::workspace_chrome_titles(&self.tabs);
        let favicons: Vec<_> = labels
            .iter()
            .map(|title| Some(crate::favicon::Favicon::fallback(title)))
            .collect();
        let url = self.current_url_text();
        let view = chrome::ShellView {
            labels: &labels,
            favicons: &favicons,
            url: &url,
            can_go_back: self.nav.can_go_back(),
            can_go_forward: self.nav.can_go_forward(),
            active: super::presentation::workspace_chrome_active(&self.tabs),
            top_css: self.chrome_mode.state(Instant::now()).band_css,
            sidebar: self.rail_open,
            hover: self.shell_hover,
            focus: self.shell_focus,
        };
        f(&view)
    }

    /// Hit-tests the unified shell at CSS coordinates.
    pub(super) fn shell_hit_at(&self, x: f32, y: f32) -> Option<chrome::ShellHit> {
        self.with_shell_view(|view| {
            chrome::shell_hit(
                self.metrics.css_width as f32,
                self.metrics.css_height as f32,
                self.metrics.scale as f32,
                view,
                x,
                y,
            )
        })
    }

    /// Tab/Shift+Tab walks `shell_focus_order` when the page has no focusables;
    /// pages with form controls keep their existing DOM focus traversal.
    pub(super) fn handle_shell_focus_key(&mut self, key: &Key, shift: bool) -> bool {
        if !matches!(key, Key::Named(NamedKey::Tab)) || !self.focus_targets.is_empty() {
            return false;
        }
        let order = self.with_shell_view(chrome::shell_focus_order);
        let next = match self.shell_focus {
            Some(current) => {
                let index = order.iter().position(|hit| *hit == current).unwrap_or(0);
                let step = if shift { order.len() - 1 } else { 1 };
                order[(index + step) % order.len()]
            }
            None => order[usize::from(shift) * (order.len() - 1)],
        };
        self.shell_focus = Some(next);
        self.request_repaint();
        true
    }

    /// Dispatches a shell hit using the same actions the legacy chrome offered.
    pub(super) fn activate_shell_hit(&mut self, hit: chrome::ShellHit) {
        self.shell_focus = Some(hit);
        match hit {
            chrome::ShellHit::ToggleSidebar => self.rail_open = !self.rail_open,
            chrome::ShellHit::Nav(button) => match button {
                NavigationButton::Back => self.go_back(),
                NavigationButton::Forward => self.go_forward(),
                NavigationButton::Reload => self.reload(),
            },
            chrome::ShellHit::Activate(index) => self.activate_tab(index),
            chrome::ShellHit::NewTab => self.open_new_tab(),
        }
        self.request_repaint();
    }
}
