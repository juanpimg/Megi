//! Offline end-to-end tests for the advanced `net` features wired into `render`:
//! cookie jars (document ↔ subresources ↔ redirects ↔ renders), CORS for cross-origin
//! subresources (non-fatal warnings) and transparent gzip/deflate decoding.
//!
//! Every test binds its own loopback `TcpListener` (`127.0.0.1:0`) and serves hand-written
//! responses. No external network, no DNS beyond loopback, no sleeps.

mod common;
mod compression;
mod cookies;
mod cors;
mod hit_regions;
mod no_cors;
