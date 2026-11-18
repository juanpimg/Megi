#![no_main]
//! Fuzz target: HTTP/1.x status line and header parsing (#1426).
//!
//! `parse_head_is_valid` drives the same parser used by the socket-backed
//! `fetch`, covering status lines, header folding/limits and invalid bytes
//! without opening a socket.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = megi_net::http::parse_head_is_valid(data);
});
