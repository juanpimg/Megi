//! T3: renderer file grants must stop traversal and symlink escapes.
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 3 (T3).
//! Spec: POSIX.1-2017 `realpath(3)` — resolve symlinks before the containment check;
//! WHATWG URL §5.3 "path" traversal segments. Public API is owned, with no borrowed
//! lifetimes crossing the crate boundary.

use megi_browser::ipc::{FileGrantTable, GrantError, MAX_GRANTED_FILE_BYTES};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "megi-grant-{}-{label}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn granted_file_resolves_inside_the_grant_root() {
    let root = TempRoot::new("ok");
    fs::write(root.path().join("note.txt"), b"ok").expect("write note");
    let table = FileGrantTable::new();
    let grant = table.grant_for(root.path()).expect("grant root");

    let resolved = table
        .resolve_within(&grant, Path::new("note.txt"))
        .expect("in-grant path must resolve");

    assert_eq!(
        resolved,
        fs::canonicalize(root.path().join("note.txt")).unwrap()
    );
}

#[test]
fn dotdot_escape_is_rejected() {
    let root = TempRoot::new("dotdot");
    let allowed = root.path().join("allowed");
    let forbidden = root.path().join("forbidden");
    fs::create_dir_all(&allowed).expect("allowed");
    fs::create_dir_all(&forbidden).expect("forbidden");
    fs::write(forbidden.join("secret.txt"), b"secret").expect("secret");
    let table = FileGrantTable::new();
    let grant = table.grant_for(&allowed).expect("grant allowed");

    assert_eq!(
        table.resolve_within(&grant, Path::new("../forbidden/secret.txt")),
        Err(GrantError::OutsideGrant)
    );
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected() {
    let root = TempRoot::new("symlink");
    let allowed = root.path().join("allowed");
    fs::create_dir_all(&allowed).expect("allowed");
    fs::write(root.path().join("secret.txt"), b"secret").expect("secret");
    std::os::unix::fs::symlink(root.path().join("secret.txt"), allowed.join("link.txt"))
        .expect("symlink out of grant");
    let table = FileGrantTable::new();
    let grant = table.grant_for(&allowed).expect("grant allowed");

    // realpath(3) resolves the link to <root>/secret.txt, which is outside the grant.
    assert_eq!(
        table.resolve_within(&grant, Path::new("link.txt")),
        Err(GrantError::OutsideGrant)
    );
}

#[test]
fn absolute_path_is_rejected() {
    let root = TempRoot::new("absolute");
    let table = FileGrantTable::new();
    let grant = table.grant_for(root.path()).expect("grant root");

    assert_eq!(
        table.resolve_within(&grant, Path::new("/etc/passwd")),
        Err(GrantError::InvalidPath)
    );
}

#[test]
fn file_larger_than_the_cap_is_rejected() {
    let root = TempRoot::new("cap");
    let path = root.path().join("huge.bin");
    fs::File::create(&path)
        .and_then(|file| file.set_len(MAX_GRANTED_FILE_BYTES + 1))
        .expect("sparse oversized file");
    let table = FileGrantTable::new();
    let grant = table.grant_for(root.path()).expect("grant root");

    assert_eq!(
        table.resolve_within(&grant, Path::new("huge.bin")),
        Err(GrantError::TooLarge(MAX_GRANTED_FILE_BYTES + 1))
    );
}
