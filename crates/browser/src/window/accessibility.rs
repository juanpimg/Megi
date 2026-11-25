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
        let focused = self.focused_dom_node();
        let Some(document) = self.document.as_deref() else {
            return;
        };
        let snapshot = crate::a11y::snapshot(document, document.root());
        let mut signature = crate::a11y_backend::snapshot_signature(&snapshot, focused);
        // Phase 1: the open rail exposes the same tablist/tab names as the strip.
        // Phase 3: only the active workspace is published, with a group label when
        // more than one workspace holds tabs.
        let chrome_titles = super::presentation::workspace_a11y_titles(&self.tabs);
        let shell_tabs: Vec<crate::chrome::ShellTabA11y> = if self.rail_open {
            crate::chrome::strip_tab_accessibility(
                &chrome_titles,
                super::presentation::workspace_chrome_active(&self.tabs),
            )
        } else {
            Vec::new()
        };
        if !shell_tabs.is_empty() {
            signature ^= shell_tabs_signature(&shell_tabs);
        }
        if force {
            self.accessibility_publisher.invalidate();
        }
        if !self.accessibility_publisher.should_publish(signature) {
            return;
        }
        // `update_if_active` returns immediately when no AT is listening (CI/headless);
        // the mapping closure is only run when a real platform adapter is active.
        if let Some(adapter) = self.accessibility_adapter.as_mut() {
            adapter.update_if_active(|| {
                if shell_tabs.is_empty() {
                    crate::a11y_backend::build_tree_update(&snapshot, focused)
                } else {
                    crate::a11y_backend::build_tree_update_with_shell_tabs(
                        &snapshot,
                        focused,
                        &shell_tabs,
                    )
                }
            });
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

    /// Routes an AccessKit action to the existing focus/navigation model through the pure
    /// resolver, so AT behaviour is the same code path the headless tests exercise.
    fn handle_accessibility_action(&mut self, request: accesskit::ActionRequest) {
        // #2136: AccessKit publishes the full packed NodeId (generation included)
        // via `accesskit_id` -> `NodeId::get()`. Rebuild it with `from_raw`, not
        // `from_index`: the latter treats the packed value as a slot index and
        // returns an invalid sentinel for every node whose generation > 0, so
        // Focus/Click actions never matched a real DOM node. An invalid/zero raw
        // value resolves to no node and the action becomes a harmless no-op.
        let Some(dom_id) = DomNodeId::from_raw(request.target_node.0 as u32) else {
            return;
        };
        let base = self.nav.current().cloned();
        let plan = resolve_accessibility_action(
            request.action,
            dom_id,
            &self.focus_targets,
            &self.forms,
            &self.links,
            base.as_ref(),
        );
        match plan {
            AtActionPlan::Focus(index) => {
                self.focus.focus(index, self.focus_targets.len());
                self.request_repaint();
            }
            AtActionPlan::ActivateLink { focus_index, url } => {
                if let Some(index) = focus_index {
                    self.focus.focus(index, self.focus_targets.len());
                }
                self.navigate_to(url);
                self.request_repaint();
            }
            AtActionPlan::Noop => {}
        }
    }
}

/// FNV-1a hash of the shell tablist/tab names, folded into the DOM signature so
/// toggling the rail or switching tabs republishes the tree.
fn shell_tabs_signature(tabs: &[crate::chrome::ShellTabA11y]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for tab in tabs {
        for byte in tab.role.as_bytes().iter().chain(tab.name.as_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= u64::from(tab.selected);
    }
    hash
}

/// A resolved AT action, ready for the window to apply. Keeping this pure makes the
/// accessibility action path verifiable without a display or a live AT bus (F7-A11Y).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AtActionPlan {
    /// Move keyboard focus to this index in the unified focus list.
    Focus(usize),
    /// Focus (when the link is focusable) and navigate to the resolved absolute URL.
    ActivateLink {
        focus_index: Option<usize>,
        url: megi_webview::net::Url,
    },
    /// The action targets no exposed node or is unsupported.
    Noop,
}

/// Resolves an AccessKit action against the shell's focus model, from the same data the
/// window uses, so AT focus/click behaviour can be tested headlessly.
pub(crate) fn resolve_accessibility_action(
    action: accesskit::Action,
    target: DomNodeId,
    focus_targets: &[FocusTarget],
    forms: &[FormControl],
    links: &[LinkRegion],
    base: Option<&megi_webview::net::Url>,
) -> AtActionPlan {
    let focus_index = focus_targets.iter().position(|focus| match focus {
        FocusTarget::Control(form_id) => forms
            .iter()
            .find(|control| control.form_id == *form_id)
            .is_some_and(|control| control.node_id == target),
        FocusTarget::Link(node) => *node == target,
    });
    match action {
        accesskit::Action::Focus => focus_index.map_or(AtActionPlan::Noop, AtActionPlan::Focus),
        accesskit::Action::Click => {
            let url = links
                .iter()
                .find(|region| region.node == Some(target))
                .and_then(|region| base.and_then(|base| resolve_href(base, &region.href).ok()));
            match url {
                Some(url) => AtActionPlan::ActivateLink { focus_index, url },
                None => focus_index.map_or(AtActionPlan::Noop, AtActionPlan::Focus),
            }
        }
        _ => AtActionPlan::Noop,
    }
}

#[cfg(test)]
mod at_action_tests {
    use super::*;
    use accesskit::Action;
    use megi_webview::core::{Document, Namespace, NodeId};
    use megi_webview::net::Url;

    fn fixture() -> (
        Document,
        NodeId,
        NodeId,
        Vec<FocusTarget>,
        Vec<FormControl>,
        Vec<LinkRegion>,
    ) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let button = doc.create_element(Namespace::Html, "button", vec![]);
        let link = doc.create_element(
            Namespace::Html,
            "a",
            vec![megi_webview::core::Attribute::new("href", "/dest")],
        );
        assert!(doc.append_child(doc.root(), html));
        assert!(doc.append_child(html, body));
        assert!(doc.append_child(body, button));
        assert!(doc.append_child(body, link));
        let forms = vec![FormControl {
            form_id: 7,
            name: String::new(),
            node_id: button,
            kind: crate::forms::FormControlKind::Button,
            rect: crate::links::CssRect::new(0.0, 0.0, 10.0, 10.0),
            value: String::new(),
            checked: false,
            disabled: false,
            options: Vec::new(),
            multiple: false,
        }];
        let links = vec![LinkRegion {
            rect: crate::links::CssRect::new(0.0, 20.0, 10.0, 10.0),
            href: "/dest".to_owned(),
            node: Some(link),
            is_blocker: false,
        }];
        let targets = build_focus_targets(&doc, &forms, &links);
        (doc, button, link, targets, forms, links)
    }

    #[test]
    fn at_focus_action_resolves_to_the_control_index() {
        let (_doc, button, _link, targets, forms, links) = fixture();
        let plan = resolve_accessibility_action(
            Action::Focus,
            button,
            &targets,
            &forms,
            &links,
            Some(&Url::parse("https://example.test/page").expect("base")),
        );
        assert_eq!(plan, AtActionPlan::Focus(0));
    }

    #[test]
    fn at_click_action_resolves_the_link_and_absolute_url() {
        let (_doc, _button, link, targets, forms, links) = fixture();
        let plan = resolve_accessibility_action(
            Action::Click,
            link,
            &targets,
            &forms,
            &links,
            Some(&Url::parse("https://example.test/page").expect("base")),
        );
        assert_eq!(
            plan,
            AtActionPlan::ActivateLink {
                focus_index: Some(1),
                url: Url::parse("https://example.test/dest").expect("url"),
            }
        );
    }

    #[test]
    fn at_actions_on_unexposed_nodes_are_noops() {
        let (_doc, _button, _link, targets, forms, links) = fixture();
        let plan = resolve_accessibility_action(
            Action::Focus,
            NodeId::from_index(9999),
            &targets,
            &forms,
            &links,
            None,
        );
        assert_eq!(plan, AtActionPlan::Noop);
    }
}
