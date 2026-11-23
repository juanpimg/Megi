//! Round 30: deterministic cooperative Web Workers (no threads).

#![cfg(feature = "js")]

#[path = "workers/basic.rs"]
mod basic;
#[path = "workers/dom_proxy.rs"]
mod dom_proxy;
#[path = "workers/helpers.rs"]
mod helpers;
