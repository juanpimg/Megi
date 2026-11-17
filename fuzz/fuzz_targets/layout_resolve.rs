#![no_main]
//! Fuzz target: layout resolution (block/inline/table/flex/grid) on untrusted DOM + CSS.

use libfuzzer_sys::fuzz_target;
use render::{RenderOptions, render_url};

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);

    // Vary viewport dimensions.
    let w = data.get(0).copied().unwrap_or(0) as u32 % 1024 + 1;
    let h = data.get(1).copied().unwrap_or(0) as u32 % 1024 + 1;
    let mut ro = RenderOptions::default();
    ro.viewport.width = w;
    ro.viewport.height = h;
    ro.no_network = true;
    // Feed the fuzzer bytes back through the data-URL parser and then the whole
    // block/inline/table/flex/grid pipeline: percent-encoding keeps the envelope
    // well formed while the decoded document is byte-for-byte the hostile payload.
    let url = format!("data:text/html,{}", percent_encode(&text));
    let _ = render_url(&url, &ro);
});

/// Percent-encodes every byte that is not an RFC 3986 unreserved character, so the
/// data-URL loader reconstructs exactly `text` instead of stopping at the first `#`.
fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}
