//! End-to-end JavaScript integration tests for the render pipeline (Phase 6, WP-6.2/6.3).
//!
//! Offline only: fixtures are rendered from `file:` URLs and external-script HTTP tests use
//! a one-shot `TcpListener` on `127.0.0.1:0`. No external network, no long sleeps.
//!
//! These tests exercise the host side: `<script>` discovery, external script loading,
//! `megi_js::JsRuntime` execution with budgets, microtask/timer draining, console capture and
//! the re-render of the mutated DOM (styles → layout → paint).

#![cfg(feature = "js")]

mod common;
mod fixtures;
mod http;
