//! Testable AT-bus surrogate for verifiable accessibility integration (F7-A11Y).

use crate::a11y::LivePoliteness;
use accesskit::TreeUpdate;
use accesskit_consumer::{FilterResult, NodeRef, Tree, TreeChangeHandler};

/// Changes a live AT consumer observes as the shell publishes successive trees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AtChangeLog {
    /// Labels of nodes announced as added.
    pub added: Vec<String>,
    /// Labels of nodes announced as updated.
    pub updated: Vec<String>,
    /// Labels of nodes announced as removed.
    pub removed: Vec<String>,
    /// Number of focus moves reported to the AT.
    pub focus_moves: usize,
    /// ARIA live-region announcements a screen reader would speak, in order.
    pub announcements: Vec<LiveAnnouncement>,
}

/// One announcement emitted by a polite or assertive live region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAnnouncement {
    pub politeness: LivePoliteness,
    pub text: String,
}

/// A deterministic AT-bus surrogate (F7-A11Y): applies the exact `TreeUpdate`s the shell
/// publishes through the real `accesskit_consumer`, recording the incremental changes a
/// platform adapter would deliver to assistive technology. This makes the "live AT" path
/// verifiable in CI without an AT-SPI/UIA bus.
#[derive(Default)]
pub struct AtBus {
    tree: Option<Tree>,
    log: AtChangeLog,
}

impl AtBus {
    pub fn new() -> Self {
        Self {
            tree: None,
            log: AtChangeLog::default(),
        }
    }

    /// Applies one published tree update the way a platform adapter does when an AT is active.
    pub fn publish(&mut self, update: TreeUpdate) {
        match &mut self.tree {
            None => {
                // On the initial tree request a live AT announces every exposed node.
                let tree = Tree::new(update, true);
                let root = tree.state().root();
                let mut stack: Vec<NodeRef<'_>> = vec![root];
                while let Some(node) = stack.pop() {
                    self.log.added.push(node_label(&node));
                    stack.extend(node.children());
                }
                // Mark the initial focus as one move for the AT.
                self.log.focus_moves += 1;
                self.tree = Some(tree);
            }
            Some(tree) => tree.update_and_process_changes(update, &mut self.log),
        }
    }

    pub fn log(&self) -> &AtChangeLog {
        &self.log
    }

    pub fn focused_role(&self) -> Option<accesskit::Role> {
        self.tree
            .as_ref()
            .and_then(|tree| tree.state().focus())
            .map(|node| node.role())
    }

    pub fn toggled_in_focus(&self) -> Option<accesskit::Toggled> {
        self.tree
            .as_ref()
            .and_then(|tree| tree.state().focus())
            .and_then(|node| node.toggled())
    }

    pub fn focus_supports_action(&self, action: accesskit::Action) -> bool {
        let include = |_n: &NodeRef<'_>| FilterResult::Include;
        self.tree
            .as_ref()
            .and_then(|tree| tree.state().focus())
            .is_some_and(|node| node.supports_action(action, &include))
    }
}

impl TreeChangeHandler for AtChangeLog {
    fn node_added(&mut self, node: &NodeRef<'_>) {
        self.added.push(node_label(node));
        self.announce(node, announcement_text(node));
    }
    fn node_updated(&mut self, old: &NodeRef<'_>, new: &NodeRef<'_>) {
        self.updated.push(node_label(new));
        let text = announcement_text(new);
        if text != announcement_text(old) {
            self.announce(new, text);
        }
    }
    fn focus_moved(&mut self, _old: Option<&NodeRef<'_>>, _new: Option<&NodeRef<'_>>) {
        self.focus_moves += 1;
    }
    fn node_removed(&mut self, node: &NodeRef<'_>) {
        self.removed.push(node_label(node));
    }
}

fn node_label(node: &NodeRef<'_>) -> String {
    match node.label() {
        Some(label) if !label.is_empty() => format!("{:?}:{}", node.role(), label),
        _ => format!("{:?}", node.role()),
    }
}

impl AtChangeLog {
    fn announce(&mut self, node: &NodeRef<'_>, text: String) {
        let politeness = match node.live() {
            accesskit::Live::Polite => LivePoliteness::Polite,
            accesskit::Live::Assertive => LivePoliteness::Assertive,
            accesskit::Live::Off => return,
        };
        if text.trim().is_empty() {
            return;
        }
        self.announcements
            .push(LiveAnnouncement { politeness, text });
    }
}

/// Text a screen reader speaks for a live-region change: the accessible name first,
/// then the value and finally the role as a deterministic fallback.
fn announcement_text(node: &NodeRef<'_>) -> String {
    node.label()
        .filter(|label| !label.trim().is_empty())
        .or_else(|| node.value().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| format!("{:?}", node.role()))
}
