#!/usr/bin/env python3
"""Check that visual-parity artifacts are covered in the audit ledger (Python only).

``audit_coverage.py --check --strict`` validates the whole ledger; this checker
focuses on the files added by the visual-parity work and verifies both their
status and their recorded line counts.

Usage:
    python3 tools/check_parity_ledger.py [--root /path/to/repo]
                                        [--ledger docs/audit/coverage.tsv]
                                        [path ...]
"""
from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

DEFAULT_PATHS = (
    "docs/DESLOP-AUDIT.md",
    "docs/VISUAL-PARITY.md",
    "tests/fixtures/visual-parity/baseline.json",
    "tools/check_parity_baseline.py",
    "tools/check_parity_docs.py",
    "tools/check_parity_ledger.py",
    "tools/check_parity_schema.py",
    "tools/test_check_parity_baseline.py",
    "tools/test_check_parity_docs.py",
    "tools/test_check_parity_ledger.py",
    "tools/test_check_parity_schema.py",
    "tools/test_visual_parity.py",
    "tools/test_visual_parity_report.py",
    "tools/visual_parity.py",
    "tools/visual_parity_report.py",
)


def count_lines(path: Path) -> int:
    """Same counting rule as tools/audit_coverage.py."""
    data = path.read_bytes()
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def read_ledger(path: Path) -> dict[str, dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return {row["path"]: row for row in csv.DictReader(handle, delimiter="\t")}


def validate(root: Path, ledger_path: Path, paths: tuple[str, ...]) -> list[str]:
    """Return problems for missing/uncovered/stale ledger rows."""
    problems: list[str] = []
    try:
        ledger = read_ledger(ledger_path)
    except OSError as error:
        return [f"cannot read ledger: {error}"]
    for relative in paths:
        row = ledger.get(relative)
        if row is None:
            problems.append(f"{relative}: missing from {ledger_path}")
            continue
        if row.get("status") != "covered":
            problems.append(f"{relative}: status is {row.get('status')!r}, expected 'covered'")
        target = root / relative
        if not target.is_file():
            problems.append(f"{relative}: file does not exist")
            continue
        recorded = row.get("lines")
        actual = count_lines(target)
        if recorded != str(actual):
            problems.append(f"{relative}: ledger says {recorded} lines, file has {actual}")
        if not row.get("auditor") or not row.get("date"):
            problems.append(f"{relative}: auditor/date missing")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", help="paths to check (default: parity artifacts)")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--ledger", type=Path, help="ledger path (default: ROOT/docs/audit/coverage.tsv)")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    ledger = args.ledger or root / "docs/audit/coverage.tsv"
    paths = tuple(args.paths) or DEFAULT_PATHS
    problems = validate(root, ledger, paths)
    for problem in problems:
        print(f"LEDGER: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"parity artifacts covered in audit ledger ({len(paths)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
