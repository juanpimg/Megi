#!/usr/bin/env python3
"""Validate the generated status JSON files (Python only, no cargo).

Every ``docs/status/*.json`` must parse as strict JSON with a non-empty object
at the top level. A small set of core reports must be present.

Usage:
    python3 tools/check_status_json.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REQUIRED = ("coverage.json", "real-sites.json", "wpt.json")


def _reject_constant(value: str) -> None:
    raise ValueError(f"non-finite JSON constant {value}")


def validate(root: Path) -> list[str]:
    """Return problems found in docs/status/*.json."""
    status = root / "docs/status"
    if not status.is_dir():
        return [f"missing directory {status}"]
    problems: list[str] = []
    found: set[str] = set()
    for path in sorted(status.glob("*.json")):
        found.add(path.name)
        try:
            data = json.loads(path.read_text(encoding="utf-8"), parse_constant=_reject_constant)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            problems.append(f"{path.name}: invalid JSON ({error})")
            continue
        if not isinstance(data, dict) or not data:
            problems.append(f"{path.name}: top level must be a non-empty object")
    for name in REQUIRED:
        if name not in found:
            problems.append(f"{name}: required status report is missing")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"STATUS JSON: {problem}", file=sys.stderr)
    if problems:
        return 1
    count = len(list((args.root.resolve() / "docs/status").glob("*.json")))
    print(f"status JSON OK ({count} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
