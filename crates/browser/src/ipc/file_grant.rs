//! Renderer file grants: canonicalize once, then enforce containment and a size cap.
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 3 (T3).
//! Spec: POSIX.1-2017 `realpath(3)` resolves symlinks and `..` before the prefix check;
//! WHATWG URL §5.3 forbids traversal segments escaping a base URL. A grant is an owned
//! `PathBuf` — no borrowed lifetimes cross the crate boundary, no `Rc`/`Weak`, no unsafe.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Largest file a renderer may resolve through a grant (64 MiB).
pub const MAX_GRANTED_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// A canonicalized directory root handed to one renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileGrant {
    root: PathBuf,
}

impl FileGrant {
    /// Canonical root that every resolved path must stay under.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Failures while granting or resolving a renderer-supplied path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantError {
    /// Path was absolute or otherwise not a relative path inside the grant.
    InvalidPath,
    /// Real path escaped the canonical grant root (including via a symlink).
    OutsideGrant,
    /// Resolved file exceeds [`MAX_GRANTED_FILE_BYTES`].
    TooLarge(u64),
    /// Grant was issued by a different table, or an I/O operation failed.
    Io(String),
}

impl std::fmt::Display for GrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath => f.write_str("path is not relative to a grant"),
            Self::OutsideGrant => f.write_str("path escapes its grant root"),
            Self::TooLarge(size) => write!(f, "granted file too large ({size} bytes)"),
            Self::Io(reason) => write!(f, "grant I/O error: {reason}"),
        }
    }
}

/// Table of directories granted to renderer processes.
///
/// A `Mutex` keeps the table usable from the IPC reader threads without `Rc`/`Weak`.
#[derive(Debug, Default)]
pub struct FileGrantTable {
    grants: Mutex<Vec<FileGrant>>,
}

impl FileGrantTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Canonicalizes `directory` and records it as a grant root.
    pub fn grant_for(&self, directory: &Path) -> Result<FileGrant, GrantError> {
        let root =
            fs::canonicalize(directory).map_err(|error| GrantError::Io(error.to_string()))?;
        if !root.is_dir() {
            return Err(GrantError::InvalidPath);
        }
        let grant = FileGrant { root };
        let mut grants = self
            .grants
            .lock()
            .map_err(|error| GrantError::Io(error.to_string()))?;
        if !grants.contains(&grant) {
            grants.push(grant.clone());
        }
        Ok(grant)
    }

    /// Resolves a relative path through symlinks and rejects escapes / oversized files.
    pub fn resolve_within(&self, grant: &FileGrant, path: &Path) -> Result<PathBuf, GrantError> {
        let grants = self
            .grants
            .lock()
            .map_err(|error| GrantError::Io(error.to_string()))?;
        if !grants.contains(grant) {
            drop(grants);
            return Err(GrantError::Io(
                "grant was not issued by this table".to_owned(),
            ));
        }
        drop(grants);
        if path.is_absolute() {
            return Err(GrantError::InvalidPath);
        }
        let resolved = fs::canonicalize(grant.root.join(path))
            .map_err(|error| GrantError::Io(error.to_string()))?;
        // realpath(3): compare the *resolved* path, never the textual one.
        if !resolved.starts_with(&grant.root) {
            return Err(GrantError::OutsideGrant);
        }
        let size = fs::metadata(&resolved)
            .map_err(|error| GrantError::Io(error.to_string()))?
            .len();
        if size > MAX_GRANTED_FILE_BYTES {
            return Err(GrantError::TooLarge(size));
        }
        Ok(resolved)
    }
}
