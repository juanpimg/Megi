#!/usr/bin/env python3
"""Report whether the conformance gate stamp is still fresh (Python only).

This is informational: it does not run the harness and does not compile.
``--strict`` turns a stale stamp into exit 1. The authoritative check remains
``tools/update_coverage.py --check``, which fails on stale gate evidence.

Usage:
    python3 tools/check_conformance_freshness.py [--root /path/to/repo] [--strict]
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import tomllib
from pathlib import Path

# #2444: single source of truth for the paths that invalidate a conformance
# gate stamp. `update_coverage.py` imports this tuple so both checks agree.
GATE_PATHS = (
    "crates/html",
    "crates/css",
    "crates/render",
    "crates/test_harness",
    "tests/html5lib-tests",
    "tests/wpt",
)


def _git(root: Path, args: list[str], run=subprocess.run):
    return run(["git", "-C", str(root), *args], capture_output=True, text=True)


def analyze(root: Path, run=subprocess.run) -> dict:
    """Return freshness information for the [gate] verified_commit stamp."""
    manifest = root / "docs/status/progress.toml"
    if not manifest.is_file():
        return {"error": "missing docs/status/progress.toml"}
    with manifest.open("rb") as handle:
        commit = str(tomllib.load(handle).get("gate", {}).get("verified_commit", "")).strip()
    if not commit:
        return {"error": "no [gate].verified_commit"}
    ancestor = _git(root, ["merge-base", "--is-ancestor", commit, "HEAD"], run)
    if ancestor.returncode != 0:
        return {"commit": commit, "stale": True, "reason": "verified_commit is not an ancestor of HEAD"}
    diff = _git(root, ["diff", "--name-only", f"{commit}..HEAD", "--", *GATE_PATHS], run)
    if diff.returncode != 0:
        return {"commit": commit, "stale": True, "reason": "git diff failed"}
    changed = sorted(line for line in diff.stdout.splitlines() if line.strip())
    return {
        "commit": commit,
        "stale": bool(changed),
        "changed_paths": changed,
        "reason": f"{len(changed)} gate-relevant paths changed since {commit}",
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args(argv)
    info = analyze(args.root.resolve())
    if "error" in info:
        print(f"CONFORMANCE FRESHNESS: {info['error']}", file=sys.stderr)
        return 1
    status = "STALE" if info["stale"] else "FRESH"
    print(f"CONFORMANCE FRESHNESS: {status} — {info['reason']}")
    for path in info.get("changed_paths", [])[:5]:
        print(f"    {path}")
    if info["stale"] and args.strict:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
