#![forbid(unsafe_code)]
//! Renderer sandbox policy selector (M3#4).
//!
//! The parent process never uses `pre_exec`: enforcement is applied by the renderer itself,
//! immediately after `execve` and before any untrusted IPC message is read. On Linux this is
//! backed by Landlock; macOS and Windows report `Unsupported` until M5.

use std::fmt;
use std::path::{Path, PathBuf};

mod linux;
mod unsupported;

/// Enforcement level actually obtained by the renderer (never inferred from the target OS).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxMode {
    /// Every requested restriction was accepted by the kernel.
    Enforced,
    /// Some restrictions were dropped by kernel compatibility handling.
    BestEffort,
    /// No OS enforcement is available in this build/target.
    Unsupported,
}

impl SandboxMode {
    /// Stable lowercase token used in IPC reports and generated evidence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::BestEffort => "best-effort",
            Self::Unsupported => "unsupported",
        }
    }
}

impl fmt::Display for SandboxMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why the renderer could not obtain a real sandbox. Displayable in reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxError {
    message: String,
}

impl SandboxError {
    /// Creates an error carrying a human-readable cause.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the cause text.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SandboxError {}

/// Applies renderer restrictions to the **current** process.
///
/// `scratch` is the only writable path granted; the running executable is granted execute.
/// `allow_network` keeps TCP/UDP unrestricted when the slot needs remote resources; otherwise
/// network rights are handled with no grant, which denies bind/connect.
pub fn apply_renderer_sandbox(
    scratch: &Path,
    allow_network: bool,
) -> Result<SandboxMode, SandboxError> {
    apply_renderer_sandbox_with_reads(scratch, allow_network, &[])
}

/// Like [`apply_renderer_sandbox`] with explicit read-only document paths supplied by the
/// parent. They are exact files, never whole directories, so other renderer scratch dirs
/// stay unreachable.
pub fn apply_renderer_sandbox_with_reads(
    scratch: &Path,
    allow_network: bool,
    reads: &[PathBuf],
) -> Result<SandboxMode, SandboxError> {
    #[cfg(target_os = "linux")]
    {
        linux::apply(scratch, allow_network, reads)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = reads;
        unsupported::apply(scratch, allow_network)
    }
}

/// Whether this kernel can deny `connect(2)` to an AF_UNIX path via Landlock.
///
/// This needs `AccessFs::Refer` (Landlock ABI V8+). On older kernels the restriction is
/// not expressible, so negative probes must report `unsupported` instead of failing.
pub fn unix_socket_connect_restriction_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::refer_right_supported()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Static, serde-friendly security matrix published in reports.
pub fn render_security_matrix() -> &'static str {
    "linux: enforced; macos: unsupported; windows: unsupported"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_is_visible_and_never_claims_macos_or_windows() {
        assert_eq!(
            render_security_matrix(),
            "linux: enforced; macos: unsupported; windows: unsupported"
        );
    }

    #[test]
    fn mode_tokens_are_stable() {
        assert_eq!(SandboxMode::Enforced.as_str(), "enforced");
        assert_eq!(SandboxMode::BestEffort.as_str(), "best-effort");
        assert_eq!(SandboxMode::Unsupported.as_str(), "unsupported");
    }
}
