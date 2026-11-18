//! Offline tests for the minimal JavaScript event loop added to `render` (WP-6.3 host):
//! virtual-time timers with delays, `DOMContentLoaded`/`load` lifecycle events, budget
//! enforcement, determinism and `--no-scripting`.
//!
//! No display, no sleeps and no network: every case renders a local `file://` fixture whose
//! scripts only mutate the DOM.

#![cfg(feature = "js")]

mod budgets;
mod cli_workers;
mod common;
mod lifecycle;
mod timers;
