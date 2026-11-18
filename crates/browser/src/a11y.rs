//! Minimal accessibility model for the shell: implicit/explicit ARIA roles, names,
//! hidden/disabled state and a deterministic Tab focus order.
//!
//! The DOM already stores every `aria-*` attribute; this module exposes a small, testable
//! snapshot (role/name/disabled/hidden) and the focus traversal used by `Tab`/`Shift+Tab`.
//! Limits: no full ARIA spec (only the common roles/attributes), no screen-reader bridge,
//! no live-region announcements and no computed accessible-name algorithm beyond
//! `aria-label` → `aria-labelledby` → text/attribute fallback.

pub(crate) use megi_webview::core::{Document, NodeId};

mod nodes;
pub use nodes::*;
mod snapshot;
pub use snapshot::*;
#[cfg(test)]
mod tests;
