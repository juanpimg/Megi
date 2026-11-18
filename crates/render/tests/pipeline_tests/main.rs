//! Offline, deterministic integration tests for the `render` pipeline.
//!
//! Every network test spins up its own `TcpListener` on `127.0.0.1:0`; there are no
//! external connections and no long sleeps (the only sleeps are the server accept poll
//! and short fetch timeouts that make a regression fail instead of hanging).

mod common;
mod http;
mod limits;
mod output;
mod urls;
