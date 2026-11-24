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
    created.all_threads(true).map_err(cause("landlock build"))
}

/// Applies the ruleset to the calling process and all its threads.
pub(super) fn apply(
    scratch: &Path,
    allow_network: bool,
    reads: &[PathBuf],
) -> Result<SandboxMode, SandboxError> {
    // Landlock sets this too; doing it first through rustix keeps the policy explicit.
    let _ = rustix::thread::set_no_new_privs(true);
    let executable = std::env::current_exe().map_err(cause("current exe"))?;

    let mut chosen = None;
    for abi in ABIS {
        match build_ruleset(abi, allow_network, scratch, &executable, reads) {
            Ok(ruleset) => {
                chosen = Some(ruleset);
                break;
            }
            Err(error) => eprintln!("landlock ABI {abi:?} unavailable: {error}"),
        }
    }
    let Some(ruleset) = chosen else {
        return Ok(SandboxMode::BestEffort);
    };
    let status = ruleset
        .restrict_self()
        .map_err(cause("landlock restrict_self"))?;
    let dumpable = set_dumpable_behavior(DumpableBehavior::NotDumpable).is_ok();

    let enforced = status.ruleset == RulesetStatus::FullyEnforced
        && status.no_new_privs
        && status.all_threads
        && dumpable;
    Ok(if enforced {
        SandboxMode::Enforced
    } else {
        SandboxMode::BestEffort
    })
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
}
