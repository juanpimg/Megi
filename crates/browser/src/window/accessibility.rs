//! Platform accessibility wiring (ADR-002): publishes the shell's deterministic
//! accessibility snapshot to the OS through the `accesskit_winit` adapter and routes
//! accessibility actions (focus/click) back into the existing UI model.

use super::*;

use megi_webview::core::NodeId as DomNodeId;

impl WindowApp {
    /// DOM id of the currently focused element (or the document root when nothing is focused).
    fn focused_dom_node(&self) -> DomNodeId {
        if let Some(target) = self
            .focus
            .focused()
            .and_then(|index| self.focus_targets.get(index))
        {
            match target {
                FocusTarget::Control(form_id) => {
                    if let Some(control) = self
                        .forms
                        .iter()
                        .find(|control| control.form_id == *form_id)
                    {
                        return control.node_id;
                    }
                }
                FocusTarget::Link(node) => return *node,
            }
        }
        self.document
            .as_ref()
            .map(|document| document.root())
            .unwrap_or_else(|| DomNodeId::from_index(0))
    }

    /// Publishes the current accessibility tree to the platform adapter (AT-SPI/UIA/NSAccessibility).
    ///
    /// `force` re-sends a full tree even if the signature is unchanged (used on AT activation).
    pub(super) fn publish_accessibility_tree(&mut self, force: bool) {
        let Some(document) = self.document.clone() else {
            return;
        };
        let focused = self.focused_dom_node();
        let snapshot = crate::a11y::snapshot(&document, document.root());
        let signature = crate::a11y_backend::snapshot_signature(&snapshot, focused);
        if force {
            self.accessibility_publisher.invalidate();
        }
        if !self.accessibility_publisher.should_publish(signature) {
            return;
        }
        // `update_if_active` returns immediately when no AT is listening (CI/headless);
        // the mapping closure is only run when a real platform adapter is active.
        if let Some(adapter) = self.accessibility_adapter.as_mut() {
            adapter.update_if_active(|| crate::a11y_backend::build_tree_update(&snapshot, focused));
        }
    }

    /// Handles an accessibility event delivered by `accesskit_winit` through the winit proxy.
    pub(super) fn handle_accessibility_event(&mut self, event: accesskit_winit::Event) {
        use accesskit_winit::WindowEvent as A11yWindowEvent;
        match event.window_event {
            A11yWindowEvent::InitialTreeRequested => self.publish_accessibility_tree(true),
            A11yWindowEvent::ActionRequested(request) => self.handle_accessibility_action(request),
            A11yWindowEvent::AccessibilityDeactivated => {
                self.accessibility_publisher.invalidate();
            }
        }
    }

    /// Routes an AccessKit action to the existing focus/navigation model.
    fn handle_accessibility_action(&mut self, request: accesskit::ActionRequest) {
        let dom_id = DomNodeId::from_index(request.target_node.0 as usize);
        let target_index = self.focus_targets.iter().position(|target| match target {
            FocusTarget::Control(form_id) => self
                .forms
                .iter()
                .find(|control| control.form_id == *form_id)
                .is_some_and(|control| control.node_id == dom_id),
            FocusTarget::Link(node) => *node == dom_id,
        });
        match request.action {
            accesskit::Action::Focus => {
                if let Some(index) = target_index {
                    self.focus.focus(index, self.focus_targets.len());
                    self.request_repaint();
                }
            }
            accesskit::Action::Click => {
                if let Some(index) = target_index {
                    self.focus.focus(index, self.focus_targets.len());
                }
                // Activate links exactly like Enter on a focused link (keyboard.rs).
                let target = self
                    .links
                    .iter()
                    .find(|region| region.node == Some(dom_id))
                    .cloned();
                if let Some(region) = target {
                    let url = self
                        .nav
                        .current()
                        .cloned()
                        .and_then(|base| resolve_href(&base, &region.href).ok());
                    if let Some(url) = url {
                        self.navigate_to(url);
                    }
                }
                self.request_repaint();
            }
            _ => {}
        }
    }
}
