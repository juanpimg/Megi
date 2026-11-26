#!/usr/bin/env python3
"""Check progress/coverage manifests against their generated reports (Python only).

Reads ``docs/status/progress.toml`` vs ``PROGRESS.md`` and
``docs/status/coverage.toml`` vs ``docs/status/coverage.html``. No cargo.

Usage:
    python3 tools/check_progress_consistency.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

PROGRESS_RE = re.compile(
    r"commit `([0-9a-f]+)`.*?\*\*([\d,]+)\*\* tests · conformance \*\*(\w+)\*\* · "
    r"\*\*(\d+)\*\* crates · \*\*([\d,]+)\*\* lines of Rust"
)
COVERAGE_RE = re.compile(r"<b>(\d+)/(\d+)</b>")
UPDATED_RE = re.compile(r"Actualizado por: (.*?)</p>")


def _read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _read_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def _number(raw: str) -> int:
    return int(raw.replace(",", ""))


def validate_progress(manifest: dict, report: str) -> list[str]:
    """Check the required manifest keys and the generated snapshot line."""
    problems: list[str] = []
    for key in ("title", "subtitle", "last_updated_by"):
        if not str(manifest.get(key, "")).strip():
            problems.append(f"progress.toml: missing {key}")
    stats = manifest.get("stats", {})
    for key in ("tests", "tests_commit", "tests_date", "crates", "rust_lines", "unsafe_forbidden_crates"):
        if key not in stats:
            problems.append(f"progress.toml: [stats] missing {key}")
    gate = manifest.get("gate", {})
    for key in ("status", "verified_commit", "tokenizer", "tree", "serializer", "css_syntax"):
        if key not in gate:
            problems.append(f"progress.toml: [gate] missing {key}")
    if not manifest.get("phase"):
        problems.append("progress.toml: no [[phase]] entries")
    match = PROGRESS_RE.search(report)
    if not match:
        return problems + ["PROGRESS.md: snapshot line is missing or malformed"]
    commit, tests, status, crates, lines = match.groups()
    expected = (
        ("tests", _number(tests), stats.get("tests")),
        ("crates", int(crates), stats.get("crates")),
        ("rust_lines", _number(lines), stats.get("rust_lines")),
        ("conformance", status, gate.get("status")),
        ("commit", commit, stats.get("tests_commit")),
    )
    for name, actual, wanted in expected:
        if actual != wanted:
            problems.append(f"PROGRESS.md: {name}={actual!r} but manifest says {wanted!r}")
    return problems


def validate_coverage(manifest: dict, report: str) -> list[str]:
    """Check capability entries and the generated headline/update stamp."""
    problems: list[str] = []
    if not str(manifest.get("last_updated_by", "")).strip():
        problems.append("coverage.toml: missing last_updated_by")
    capabilities = manifest.get("capability", [])
    if not capabilities:
        problems.append("coverage.toml: no [[capability]] entries")
    for index, capability in enumerate(capabilities):
        for key in ("id", "name", "state", "evidence"):
            if not str(capability.get(key, "")).strip():
                problems.append(f"coverage.toml: capability[{index}] missing {key}")
    done = sum(1 for item in capabilities if item.get("state") == "done")
    match = COVERAGE_RE.search(report)
    if not match:
        problems.append("coverage.html: headline X/Y is missing")
    else:
        shown_done, shown_total = (int(value) for value in match.groups())
        if (shown_done, shown_total) != (done, len(capabilities)):
            problems.append(
                f"coverage.html: shows {shown_done}/{shown_total}, manifest has {done}/{len(capabilities)}"
            )
    stamp = UPDATED_RE.search(report)
    if not stamp:
        problems.append("coverage.html: update stamp is missing")
    elif stamp.group(1).strip() != str(manifest.get("last_updated_by", "")).strip():
        problems.append("coverage.html: update stamp differs from coverage.toml")
    return problems


def validate(root: Path) -> list[str]:
    problems: list[str] = []
    try:
        progress_manifest = _read_toml(root / "docs/status/progress.toml")
        coverage_manifest = _read_toml(root / "docs/status/coverage.toml")
        progress_report = _read_text(root / "PROGRESS.md")
        coverage_report = _read_text(root / "docs/status/coverage.html")
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        return [f"cannot read status files: {error}"]
    return (
        validate_progress(progress_manifest, progress_report)
        + validate_coverage(coverage_manifest, coverage_report)
    )


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"STATUS CONSISTENCY: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("progress/coverage consistency OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
