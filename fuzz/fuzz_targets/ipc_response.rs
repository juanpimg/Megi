#![no_main]
//! Fuzz target: untrusted IPC render response JSON decoding and semantic validation.

use libfuzzer_sys::fuzz_target;
use megi_browser::ipc::{IpcRenderResponse, validate_response};

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 {
        return;
    }
    if let Ok(resp) = serde_json::from_slice::<IpcRenderResponse>(data) {
        let _ = validate_response(resp, 0);
    }
});
