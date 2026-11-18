//! Persistent browser session: tabs, per-tab history, zoom and scroll, stored as JSON.
//!
//! The session file lives in the user data directory (`$XDG_DATA_HOME/megi/session.json`,
//! falling back to `~/.local/share/megi/session.json`), or in an explicit path from
//! `--session-file PATH`. `--no-session` disables persistence and
//! headless mode never writes a session (documented). The format is a **small self-contained
//! JSON document** (no external dependencies), versioned with [`SCHEMA_VERSION`]:
//!
//! ```json
//! {"version":1,"active":0,"tabs":[{"url":"https://example.com/","history":["https://example.com/"],"index":0,"zoom":1.0,"scroll":0}]}
//! ```
//!
//! Loading is deliberately tolerant: a missing file is a fresh session (no warning), a
//! corrupt/unsupported file produces a warning and a fresh session (never a panic), unknown
//! fields are ignored and out-of-range values are clamped. Saves write a unique
//! per-process temporary file and rename it atomically, so a crash leaves the previous
//! session intact and two instances cannot tear each other's JSON (last writer wins); the
//! advisory [`SessionLock`] sidecar is available but not wired into [`save`].
//!
//! Limits: URLs/history/zoom/scroll only (no cookies, passwords, form state or find state),
//! at most [`MAX_SESSION_TABS`] tabs and [`MAX_HISTORY_PER_TAB`] history entries per tab.

pub(crate) use std::fs;
pub(crate) use std::io::{self, Read, Write};
pub(crate) use std::path::{Path, PathBuf};

pub(crate) use crate::navigation::NavigationState;
pub(crate) use crate::scroll::ScrollState;
pub(crate) use crate::tabs::{MAX_TABS, Tab, TabsState};

mod model;
pub use model::*;
mod storage;
pub use storage::*;
mod lock;
pub use lock::*;
mod json;
pub(crate) use json::*;
#[cfg(test)]
mod tests_a;
#[cfg(test)]
mod tests_b;
