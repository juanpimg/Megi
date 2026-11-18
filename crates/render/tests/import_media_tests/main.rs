//! `@import` + media-query + showcase-golden integration tests for the render pipeline.
//!
//! Every network test spins up its own loopback `TcpListener`; the showcase is also
//! rendered from `file://` and compared byte-for-byte with the versioned golden PNG.

mod caps;
mod common;
mod imports;
mod media;
mod showcase;
