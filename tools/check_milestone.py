"""Verify that a depth work package's acceptance criterion actually closed.

Usage: python3 tools/check_milestone.py M0 1
Exit 0 only when criterion N of milestone M is no longer pending in
docs/status/progress.toml and has been recorded as verified with evidence.
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

DEFAULT_ROOT = Path(__file__).resolve().parent.parent


def check(milestone: str, index: int, root: Path | None = None) -> int:
    root = root or DEFAULT_ROOT
    path = root / "docs/status/progress.toml"
    if not path.is_file():
        print(f"FAIL: {path} not found")
        return 1
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    phase = next((p for p in data.get("phase", []) if p.get("id", "").upper() == milestone.upper()), None)
    if phase is None:
        print(f"FAIL: milestone {milestone} not found")
        return 1
    pending = phase.get("pending", [])
    items = phase.get("items", [])
    if index < 1 or index > len(pending) + len(items):
        print(f"FAIL: criterion {milestone}#{index} does not exist")
        return 1
    # A criterion is closed when it has moved out of `pending` into `items`.
    if index <= len(items):
        evidence = items[index - 1]
        if not isinstance(evidence, str) or not evidence.strip():
            print(f"FAIL: {milestone}#{index} has no recorded evidence")
            return 1
        print(f"PASS: {milestone}#{index} verified: {evidence[:140]}")
        return 0
    pending_index = index - len(items)
    if pending_index <= len(pending):
        evidence = pending[pending_index - 1]
        if not isinstance(evidence, str) or not evidence.strip():
            print(f"FAIL: {milestone}#{index} has no recorded evidence")
            return 1
        print(f"FAIL: {milestone}#{index} still pending: {evidence[:140]}")
    else:
        print(f"FAIL: {milestone}#{index} has no recorded evidence")
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check one milestone acceptance criterion")
    parser.add_argument("milestone")
    parser.add_argument("index", type=int)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = parser.parse_args(argv)
    return check(args.milestone, args.index, args.root)


if __name__ == "__main__":
    raise SystemExit(main())
