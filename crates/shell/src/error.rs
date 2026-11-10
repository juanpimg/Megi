//! Typed errors for the shell/CLI.

use std::fmt;

/// Everything that can go wrong launching Megi or taking a headless screenshot.
#[derive(Debug)]
pub enum ShellError {
    /// Command-line usage error (CLI exits with code 2).
    Usage(String),
    /// The render pipeline failed (bad URL, network error, limits, ...).
    Render(render::RenderError),
    /// Reading/writing a local file failed.
    Io { path: String, reason: String },
    /// Window/event-loop/softbuffer setup failed (e.g. no display server).
    Window(String),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::Render(error) => write!(f, "render error: {error}"),
            Self::Io { path, reason } => write!(f, "I/O error on {path}: {reason}"),
            Self::Window(message) => write!(f, "window error: {message}"),
        }
    }
}

impl std::error::Error for ShellError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            _ => None,
        }
    }
}

impl From<render::RenderError> for ShellError {
    fn from(error: render::RenderError) -> Self {
        Self::Render(error)
    }
}
