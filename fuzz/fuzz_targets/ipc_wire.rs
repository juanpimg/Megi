#![no_main]
//! Fuzz target: versioned and legacy IPC wire framing decoders.

use libfuzzer_sys::fuzz_target;
use megi_browser::ipc::{IpcRenderRequest, read_framed_envelope, read_message_checked};

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    let mut framed_cursor = data;
    let _ = read_framed_envelope::<_, IpcRenderRequest>(&mut framed_cursor);

    let mut legacy_cursor = data;
    let _ = read_message_checked::<_, IpcRenderRequest>(&mut legacy_cursor);
});
