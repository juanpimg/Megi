#!/usr/bin/env python3
"""Validate versioned work-package TOML files (Python only).

Usage:
    python3 tools/check_work_packages.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

REQUIRED = (
    "id", "source", "file", "expected", "actual", "spec",
    "acceptance", "verify_cmd", "metric", "milestone",
)
QUEUE = Path("docs/work-packages/queue")


def validate(queue: Path) -> list[str]:
    """Return schema problems from every ``*.toml`` in the queue."""
    if not queue.is_dir():
        return [f"missing directory {queue}"]
    files = sorted(queue.glob("*.toml"))
    if not files:
        return ["no work packages found"]
    problems: list[str] = []
    ids: set[str] = set()
    for path in files:
        label = path.name
        try:
            with path.open("rb") as handle:
                data = tomllib.load(handle)
        except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
            problems.append(f"{label}: invalid TOML ({error})")
            continue
        for key in REQUIRED:
            value = data.get(key)
            if not isinstance(value, str) or not value.strip():
                problems.append(f"{label}: missing or empty {key}")
        pkg_id = data.get("id")
        if isinstance(pkg_id, str) and pkg_id:
            if pkg_id in ids:
                problems.append(f"{label}: duplicate id {pkg_id}")
            ids.add(pkg_id)
            if pkg_id != path.stem:
                problems.append(f"{label}: id {pkg_id} does not match file name")
        for key in ("impact", "frequency"):
            if key in data and (not isinstance(data[key], int) or data[key] < 1):
                problems.append(f"{label}: {key} must be a positive integer")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    problems = validate(root / QUEUE)
    for problem in problems:
        print(f"WORK PACKAGE: {problem}", file=sys.stderr)
    if problems:
        return 1
    count = len(list((root / QUEUE).glob("*.toml")))
    print(f"work packages OK ({count} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
