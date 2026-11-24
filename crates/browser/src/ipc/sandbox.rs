//! M3#4: worker-side sandbox setup and the enforce level reported to the parent.
//!
//! The parent only prepares the scratch cwd; the child decides its own mode, so no
//! unsafe `pre_exec` hook is involved. In-memory stream tests never call this module.

use super::IpcRenderRequest;
use megi_sandbox::SandboxMode;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static RENDERER_SANDBOX_MODE: OnceLock<SandboxMode> = OnceLock::new();

/// Applies the policy to the calling process before it reads any IPC frame. The parent
/// passes the exact document file as a read-only grant; no whole directory is widened.
pub(super) fn enforce_worker_sandbox(scratch: &Path, allow_network: bool) {
    let reads: Vec<PathBuf> = std::env::var_os("MEGI_SANDBOX_READ_PATH")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    match megi_sandbox::apply_renderer_sandbox_with_reads(scratch, allow_network, &reads) {
        Ok(mode) => {
            let _ = RENDERER_SANDBOX_MODE.set(mode);
        }
        Err(error) => eprintln!("sandbox unavailable: {error}"),
    }
}

/// Level visible in `IpcRenderResponse`; `unsupported` when no worker was sandboxed.
pub(super) fn reported_sandbox_mode() -> &'static str {
    RENDERER_SANDBOX_MODE
        .get()
        .map(|mode| mode.as_str())
        .unwrap_or("unsupported")
}

/// Network policy decided per slot before the renderer is spawned (M3#4): network is
/// needed only when the effective document URL uses a network scheme. Data/file documents
/// get a Landlock network deny. `base_override` wins for patched DOM, as with site keys.
pub fn request_sandbox_allows_network(request: &IpcRenderRequest) -> bool {
    let url = request.base_override.as_deref().unwrap_or(&request.input);
    url.starts_with("http://") || url.starts_with("https://")
}

/// Exact local document file the worker may read, when the request names one. Only an
/// existing plain path qualifies: URLs and inline documents need no filesystem grant, and
/// no enclosing directory is ever widened.
pub fn request_sandbox_read_path(request: &IpcRenderRequest) -> Option<PathBuf> {
    let input = request.base_override.as_deref().unwrap_or(&request.input);
    let plain = match input.strip_prefix("file://") {
        Some(url_path) => percent_decode(url_path),
        None if input.contains("://") || input.starts_with("data:") => return None,
        None => input.to_owned(),
    };
    let path = PathBuf::from(plain);
    path.is_file().then_some(path)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            out.push((hi * 16 + lo) as u8);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
