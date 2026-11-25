#![forbid(unsafe_code)]
#![cfg(target_os = "linux")]
//! Linux enforcement with the safe `landlock` abstraction (M3#4).
//!
//! Rules are installed by the renderer on itself after `execve`; the parent never uses
//! `pre_exec`. `no_new_privs` and `dumpable(0)` come from safe `rustix` APIs.

use crate::{SandboxError, SandboxMode};
use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, PathBeneath, PathFd,
    RestrictSelfAttr, Ruleset, RulesetAttr, RulesetCreated, RulesetCreatedAttr, RulesetStatus,
};
use rustix::process::{DumpableBehavior, set_dumpable_behavior};
use std::fmt::Display;
use std::path::{Path, PathBuf};

const ABIS: [ABI; 10] = [
    ABI::V10,
    ABI::V9,
    ABI::V8,
    ABI::V7,
    ABI::V6,
    ABI::V5,
    ABI::V4,
    ABI::V3,
    ABI::V2,
    ABI::V1,
];

fn cause<E: Display>(context: &'static str) -> impl FnOnce(E) -> SandboxError {
    move |error| SandboxError::new(format!("{context}: {error}"))
}

/// Builds a ruleset with `HardRequirement`: any silently dropped right is an error, so
/// `Enforced` can only be reported when every requested restriction is real.
fn build_ruleset(
    abi: ABI,
    allow_network: bool,
    scratch: &Path,
    executable: &Path,
    reads: &[PathBuf],
    all_threads: bool,
) -> Result<RulesetCreated, SandboxError> {
    let file_rights = AccessFs::from_all(abi);
    let scratch_rule = PathBeneath::new(
        PathFd::new(scratch).map_err(cause("scratch path fd"))?,
        file_rights,
    );
    let exec_rule = PathBeneath::new(
        PathFd::new(executable).map_err(cause("executable path fd"))?,
        AccessFs::from_file(abi),
    );
    let mut builder = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(file_rights)
        .map_err(cause("landlock fs rights"))?;
    if !allow_network {
        let net_rights = AccessNet::from_all(abi);
        if net_rights.is_empty() {
            return Err(SandboxError::new(
                "kernel ABI cannot restrict network access",
            ));
        }
        builder = builder
            .handle_access(net_rights)
            .map_err(cause("landlock net rights"))?;
    }
    let mut created = builder
        .create()
        .and_then(|ruleset| ruleset.add_rule(scratch_rule))
        .and_then(|ruleset| ruleset.add_rule(exec_rule))
        .map_err(cause("landlock build"))?;
    for path in reads {
        let rights = AccessFs::from_read(abi) & AccessFs::from_file(abi);
        let rule = PathBeneath::new(
            PathFd::new(path).map_err(cause("document path fd"))?,
            rights,
        );
        created = created.add_rule(rule).map_err(cause("document rule"))?;
    }
    if all_threads {
        created = created
            .all_threads(true)
            .map_err(cause("landlock all_threads"))?;
    }
    Ok(created)
}

/// Landlock ABI V8 added `AccessFs::Refer`, which is what denies `connect(2)` to an
/// existing AF_UNIX socket path. Older kernels cannot express that restriction; callers
/// must report the probe as unsupported instead of claiming a sandbox failure.
pub(super) fn refer_right_supported() -> bool {
    match Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(ABI::V8))
    {
        Ok(ruleset) => ruleset.create().is_ok(),
        Err(_) => false,
    }
}

/// Decides whether the obtained restriction is a real sandbox. `all_threads` is requested
/// only when the kernel can grant it; the renderer applies the policy before spawning any
/// worker thread, so a fully enforced single-thread ruleset is still `Enforced` (M3#4).
fn enforcement_mode(
    all_threads_requested: bool,
    ruleset_fully_enforced: bool,
    no_new_privs: bool,
    all_threads: bool,
    dumpable: bool,
) -> SandboxMode {
    let thread_scope_ok = !all_threads_requested || all_threads;
    if ruleset_fully_enforced && no_new_privs && thread_scope_ok && dumpable {
        SandboxMode::Enforced
    } else {
        SandboxMode::BestEffort
    }
}

/// Applies the ruleset to the calling process, and to all its threads when the kernel
/// supports Landlock ABI V8's `TSYNC` flag.
pub(super) fn apply(
    scratch: &Path,
    allow_network: bool,
    reads: &[PathBuf],
) -> Result<SandboxMode, SandboxError> {
    // Landlock sets this too; doing it first through rustix keeps the policy explicit.
    let _ = rustix::thread::set_no_new_privs(true);
    let executable = std::env::current_exe().map_err(cause("current exe"))?;

    // ABI V8 added `all_threads(true)`. With `HardRequirement` a pre-V8 kernel rejects it
    // instead of silently restricting a single thread, so retry without TSYNC: at this
    // point the renderer has no other thread, and the full ruleset is still enforced.
    let mut chosen = None;
    for all_threads in [true, false] {
        for abi in ABIS {
            match build_ruleset(abi, allow_network, scratch, &executable, reads, all_threads) {
                Ok(ruleset) => {
                    chosen = Some((ruleset, all_threads));
                    break;
                }
                Err(error) => {
                    eprintln!("landlock ABI {abi:?} all_threads={all_threads} unavailable: {error}")
                }
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    let Some((ruleset, all_threads)) = chosen else {
        return Ok(SandboxMode::BestEffort);
    };
    let status = ruleset
        .restrict_self()
        .map_err(cause("landlock restrict_self"))?;
    let dumpable = set_dumpable_behavior(DumpableBehavior::NotDumpable).is_ok();

    Ok(enforcement_mode(
        all_threads,
        status.ruleset == RulesetStatus::FullyEnforced,
        status.no_new_privs,
        status.all_threads,
        dumpable,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_probe_is_ordered_and_rights_are_non_empty() {
        assert_eq!(ABIS[0], ABI::V10);
        assert!(!AccessFs::from_all(ABI::V10).is_empty());
        assert!(!AccessNet::from_all(ABI::V10).is_empty());
    }

    #[test]
    fn enforcement_accepts_single_thread_ruleset_on_pre_v8_kernels() {
        // CI runners land on Landlock ABI < V8, where `all_threads(true)` cannot be granted.
        // The renderer is single-threaded at apply time, so a fully enforced ruleset on the
        // current thread is a real sandbox and must still be reported as enforced.
        assert_eq!(
            enforcement_mode(false, true, true, false, true),
            SandboxMode::Enforced
        );
        assert_eq!(
            enforcement_mode(true, true, true, false, true),
            SandboxMode::BestEffort
        );
    }
}
