#![forbid(unsafe_code)]
#![cfg(not(target_os = "linux"))]
//! macOS/Windows: no FFI, no pretend enforcement (M3#4 → M5).

use crate::{SandboxError, SandboxMode};
use std::path::Path;

pub(super) fn apply(_scratch: &Path, _allow_network: bool) -> Result<SandboxMode, SandboxError> {
    Ok(SandboxMode::Unsupported)
}
