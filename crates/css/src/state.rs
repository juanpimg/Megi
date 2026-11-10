//! Interaction/document state for dynamic pseudo-classes (`:focus`, `:hover`, `:checked`, ...).
//!
//! The state is supplied by the host (shell/render) explicitly through [`StateContext`], and
//! is also read from `data-megi-*` attributes so static documents and golden fixtures can model
//! interaction without a live session:
//!
//! | pseudo-class | host model |
//! |---|---|
//! | `:focus`, `:focus-visible` | `data-megi-focused` / `data-megi-focus-visible` (or context) |
//! | `:hover`, `:active` | `data-megi-hover` / `data-megi-active` (or context) |
//! | `:checked` | `checked` attribute on `input`/`option`, or `data-megi-checked` |
//! | `:disabled`/`:enabled` | form control + `disabled`/`data-megi-disabled` attribute |
//! | `:read-only`/`:read-write` | form control + `readonly`/`data-megi-readonly` attribute |
//! | `:required` | form control + `required` attribute |
//! | `:placeholder-shown` | `input`/`textarea` + `placeholder` and empty value, or `data-megi-placeholder-shown` |
//!
//! The context is stored per thread while resolving/matching (so the existing selector API is
//! unchanged); [`with_state`] scopes it, and [`matches_with_state`](crate::query::matches_with_state)
//! wraps the common case. `hover`/`active` have no meaning in a headless render without a
//! pointer: the host must set the `data-megi-*` attributes or the context.

use std::cell::Cell;

use dom::{Document, NodeId};

/// Interaction state supplied by the host for dynamic pseudo-class matching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateContext {
    /// Element that currently has focus (`:focus`).
    pub focused: Option<NodeId>,
    /// Element that should show a focus ring (`:focus-visible`; falls back to `focused`).
    pub focus_visible: Option<NodeId>,
    /// Element under the pointer (`:hover`); `None` in headless rendering.
    pub hovered: Option<NodeId>,
    /// Element being pressed (`:active`); `None` in headless rendering.
    pub active: Option<NodeId>,
}

impl StateContext {
    /// Empty state (no element is focused/hovered/active).
    pub const fn new() -> Self {
        Self {
            focused: None,
            focus_visible: None,
            hovered: None,
            active: None,
        }
    }
}

thread_local! {
    static CURRENT: Cell<StateContext> = const { Cell::new(StateContext::new()) };
}

/// Returns the state active on this thread (empty by default).
pub fn current() -> StateContext {
    CURRENT.with(Cell::get)
}

/// Runs `f` with `state` as the current interaction state, restoring the previous value.
pub fn with_state<T>(state: StateContext, f: impl FnOnce() -> T) -> T {
    let previous = CURRENT.with(|cell| cell.replace(state));
    struct Restore(StateContext);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(previous);
    f()
}

/// Scoped state guard: use [`with_state`] unless RAII is more convenient.
pub struct StateGuard(StateContext);

impl StateGuard {
    pub fn new(state: StateContext) -> Self {
        Self(CURRENT.with(|cell| cell.replace(state)))
    }
}

impl Drop for StateGuard {
    fn drop(&mut self) {
        CURRENT.with(|cell| cell.set(self.0));
    }
}

fn attribute<'a>(doc: &'a Document, node: NodeId, name: &str) -> Option<&'a str> {
    doc.node(node)
        .as_element()
        .and_then(|element| element.get_attribute(name))
}

pub(crate) fn has_attribute(doc: &Document, node: NodeId, name: &str) -> bool {
    attribute(doc, node, name).is_some()
}

fn element_name(doc: &Document, node: NodeId) -> Option<&str> {
    doc.node(node)
        .as_element()
        .map(|element| element.local_name.as_str())
}

pub(crate) fn is_form_control(doc: &Document, node: NodeId) -> bool {
    element_name(doc, node).is_some_and(|name| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "input" | "button" | "select" | "textarea" | "option" | "optgroup" | "fieldset"
        )
    })
}

fn is_disabled_attr(doc: &Document, node: NodeId) -> bool {
    has_attribute(doc, node, "disabled")
        || has_attribute(doc, node, "data-megi-disabled")
        || doc.node(node).parent.is_some_and(|parent| {
            element_name(doc, parent).is_some_and(|name| name.eq_ignore_ascii_case("fieldset"))
                && has_attribute(doc, parent, "disabled")
        })
}

fn editable_control(doc: &Document, node: NodeId) -> bool {
    element_name(doc, node)
        .is_some_and(|name| matches!(name.to_ascii_lowercase().as_str(), "input" | "textarea"))
}

fn read_only_attr(doc: &Document, node: NodeId) -> bool {
    has_attribute(doc, node, "readonly")
        || has_attribute(doc, node, "data-megi-readonly")
        || attribute(doc, node, "contenteditable").is_some_and(|v| v.eq_ignore_ascii_case("false"))
}

fn required_attr(doc: &Document, node: NodeId) -> bool {
    has_attribute(doc, node, "required") || has_attribute(doc, node, "data-megi-required")
}

fn checked_attr(doc: &Document, node: NodeId) -> bool {
    has_attribute(doc, node, "checked")
        || has_attribute(doc, node, "data-megi-checked")
        || attribute(doc, node, "aria-checked")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn placeholder_shown(doc: &Document, node: NodeId) -> bool {
    if has_attribute(doc, node, "data-megi-placeholder-shown") {
        return true;
    }
    if !editable_control(doc, node) || !has_attribute(doc, node, "placeholder") {
        return false;
    }
    attribute(doc, node, "value")
        .or_else(|| attribute(doc, node, "data-value"))
        .is_none_or(str::is_empty)
}

/// `:focus`
pub(crate) fn is_focused(doc: &Document, node: NodeId) -> bool {
    match current().focused {
        // An explicit host focus is authoritative; attributes model state in static docs.
        Some(focused) => focused == node,
        None => has_attribute(doc, node, "data-megi-focused"),
    }
}

/// `:focus-visible` (falls back to `:focus`).
pub(crate) fn is_focus_visible(doc: &Document, node: NodeId) -> bool {
    let state = current();
    match state.focus_visible {
        Some(visible) => visible == node,
        None => match state.focused {
            Some(focused) => focused == node,
            None => has_attribute(doc, node, "data-megi-focus-visible") || is_focused(doc, node),
        },
    }
}

/// `:hover` — no pointer in headless rendering; host marks `data-megi-hover`/context.
pub(crate) fn is_hovered(doc: &Document, node: NodeId) -> bool {
    match current().hovered {
        Some(hovered) => hovered == node,
        None => has_attribute(doc, node, "data-megi-hover"),
    }
}

/// `:active` — no pointer in headless rendering; host marks `data-megi-active`/context.
pub(crate) fn is_active(doc: &Document, node: NodeId) -> bool {
    match current().active {
        Some(active) => active == node,
        None => has_attribute(doc, node, "data-megi-active"),
    }
}

/// `:checked`
pub(crate) fn is_checked(doc: &Document, node: NodeId) -> bool {
    checked_attr(doc, node)
}

/// `:disabled`
pub(crate) fn is_disabled(doc: &Document, node: NodeId) -> bool {
    is_form_control(doc, node) && is_disabled_attr(doc, node)
}

/// `:enabled`
pub(crate) fn is_enabled(doc: &Document, node: NodeId) -> bool {
    is_form_control(doc, node) && !is_disabled_attr(doc, node)
}

/// `:read-only`
pub(crate) fn is_read_only(doc: &Document, node: NodeId) -> bool {
    if editable_control(doc, node) {
        read_only_attr(doc, node)
    } else {
        // Non-editable elements are read-only by definition (Selectors 4 §9.4).
        !has_attribute(doc, node, "contenteditable")
    }
}

/// `:read-write`
pub(crate) fn is_read_write(doc: &Document, node: NodeId) -> bool {
    editable_control(doc, node) && !read_only_attr(doc, node)
}

/// `:required`
pub(crate) fn is_required(doc: &Document, node: NodeId) -> bool {
    is_form_control(doc, node) && required_attr(doc, node)
}

/// `:placeholder-shown`
pub(crate) fn is_placeholder_shown(doc: &Document, node: NodeId) -> bool {
    placeholder_shown(doc, node)
}
