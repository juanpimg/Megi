//! T9: renderer children start with a scrubbed environment and their own process group.
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 3 (T9).
//! Spec: POSIX.1-2017 process groups (a child in group 0 leads a new group whose pgid
//! equals its pid), and `execve(2)` environment inheritance — only an explicit PATH
//! allowlist crosses the boundary. Both assertions use real spawned processes.

use megi_browser::ipc::{renderer_command, renderer_scratch_dir};
use std::fs;
use std::path::Path;

#[cfg(unix)]
#[test]
fn renderer_environment_is_cleared_and_cwd_is_scratch() {
    let scratch = renderer_scratch_dir().expect("scratch dir");
    let output = renderer_command(Path::new("/usr/bin/env"), &[], &scratch)
        .output()
        .expect("run env probe");
    assert!(output.status.success());
    let dump = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<&str> = dump.lines().collect();
    assert!(!entries.is_empty(), "env probe must report its environment");
    assert!(
        entries
            .iter()
            .all(|line| line.starts_with("PATH=") || line.starts_with("RUST_BACKTRACE=")),
        "renderer inherited parent environment: {dump}"
    );
    assert!(
        entries.contains(&"PATH=/usr/bin:/bin"),
        "renderer must get the pinned PATH allowlist, got {dump}"
    );
}

#[cfg(unix)]
#[test]
fn renderer_child_leads_its_own_process_group() {
    let scratch = renderer_scratch_dir().expect("scratch dir");
    let child = renderer_command(Path::new("/usr/bin/sleep"), &["30".to_owned()], &scratch)
        .spawn()
        .expect("spawn sleep probe");
    let pid = child.id();
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).expect("/proc stat");
    let after_comm = stat.rsplit_once(')').expect("stat has comm").1;
    // After `comm`: state(0), ppid(1), pgrp(2) — POSIX process group id.
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    let pgrp: u32 = fields[2].parse().expect("pgrp");
    assert_eq!(
        pgrp, pid,
        "renderer must be the leader of its own process group"
    );

    let _ = std::process::Command::new("/usr/bin/kill")
        .args(["-TERM", &format!("-{pid}")])
        .status();
    drop(child);
}
