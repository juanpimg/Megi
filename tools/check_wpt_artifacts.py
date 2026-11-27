#!/usr/bin/env python3
"""Check WPT artifact consistency (Python only, no harness or cargo).

Validates the pinned revision, the vendored manifest SHA, the gate baseline,
the generated report and the [wpt] block in the progress manifest.

Usage:
    python3 tools/check_wpt_artifacts.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

REVISION = Path("tests/wpt/WPT-REVISION.json")
MANIFEST = Path("tests/wpt/MANIFEST.json")
BASELINE = Path("tests/wpt/baseline.json")
REPORT = Path("docs/status/wpt.json")
PROGRESS = Path("docs/status/progress.toml")
HEX40 = re.compile(r"^[0-9a-f]{40}$")


def _json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def validate(root: Path) -> list[str]:
    """Return inconsistencies among the WPT artifacts."""
    paths = (REVISION, MANIFEST, BASELINE, REPORT, PROGRESS)
    missing = [str(path) for path in paths if not (root / path).is_file()]
    if missing:
        return [f"missing artifact {path}" for path in missing]
    problems: list[str] = []
    revision = _json(root / REVISION)
    sha = revision.get("revision", "")
    if not HEX40.match(sha):
        problems.append(f"WPT-REVISION.json: invalid revision {sha!r}")
    baseline = _json(root / BASELINE)
    epoch = baseline.get("epoch", {})
    if epoch.get("wpt_revision") != sha:
        problems.append("baseline epoch revision differs from WPT-REVISION.json")
    expected_sha = hashlib.sha256((root / MANIFEST).read_bytes()).hexdigest()
    if epoch.get("manifest_sha256") != expected_sha:
        problems.append("baseline epoch manifest_sha256 does not match MANIFEST.json")
    passed, total = baseline.get("passed"), baseline.get("total")
    if type(passed) is not int or type(total) is not int:
        problems.append("baseline passed/total must be integers")
    elif not 0 <= passed <= total:
        problems.append("baseline passed/total are inconsistent")
    if not isinstance(baseline.get("allowlist"), list):
        problems.append("baseline allowlist must be a list")

    report = _json(root / REPORT)
    if report.get("schema") != "megi-wpt-report/1":
        problems.append(f"wpt.json: unexpected schema {report.get('schema')!r}")
    if report.get("epoch") != epoch:
        problems.append("wpt.json epoch differs from the baseline epoch")
    counts = report.get("counts", {})
    if any(type(value) is not int for value in counts.values()):
        problems.append("wpt.json counts must be integers")
    elif sum(counts.values()) != total:
        problems.append("wpt.json counts do not add up to the baseline total")

    with (root / PROGRESS).open("rb") as handle:
        wpt = tomllib.load(handle).get("wpt", {})
    for key in ("revision", "manifest_sha256", "passed", "total", "allowlist"):
        value = wpt.get(key)
        expected = {
            "revision": sha,
            "manifest_sha256": expected_sha,
            "passed": baseline.get("passed"),
            "total": baseline.get("total"),
            "allowlist": len(baseline.get("allowlist", [])),
        }[key]
        if key in ("passed", "total", "allowlist") and type(value) is not int:
            problems.append(f"progress.toml [wpt].{key} must be an integer")
        elif value != expected:
            problems.append(f"progress.toml [wpt].{key}={value!r}, expected {expected!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WPT ARTIFACT: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("WPT artifacts consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
