#!/usr/bin/env python3
"""Check docs/work-packages/closed.toml schema and commit stamps (Python only).

Usage:
    python3 tools/check_closed_work_packages.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

CLOSED = Path("docs/work-packages/closed.toml")
REQUIRED = ("id", "metric", "commit")
HEX = re.compile(r"^[0-9a-f]{7,40}$")


def validate(data: dict, ancestor_check=None) -> list[str]:
    problems: list[str] = []
    tables = {key: value for key, value in data.items() if isinstance(value, dict)}
    if not tables:
        return ["closed.toml has no closed work packages"]
    seen: set[str] = set()
    for table_id, entry in tables.items():
        if not re.fullmatch(r"WP-[A-Za-z0-9._-]+", table_id):
            problems.append(f"{table_id}: invalid table key")
        for key in REQUIRED:
            value = entry.get(key)
            if not isinstance(value, str) or not value.strip():
                problems.append(f"{table_id}: missing or empty {key}")
        identifier = str(entry.get("id", ""))
        if identifier != table_id:
            problems.append(f"{table_id}: id {identifier!r} does not match table key")
        if identifier in seen:
            problems.append(f"{table_id}: duplicate id")
        seen.add(identifier)
        commit = str(entry.get("commit", ""))
        if commit and HEX.match(commit) and ancestor_check is not None:
            if not ancestor_check(commit):
                problems.append(f"{table_id}: commit {commit} is not an ancestor of HEAD")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    path = root / CLOSED
    if not path.is_file():
        print(f"CLOSED WP: missing {CLOSED}", file=sys.stderr)
        return 1
    with path.open("rb") as handle:
        data = tomllib.load(handle)

    def is_ancestor(commit: str) -> bool:
        result = subprocess.run(
            ["git", "-C", str(root), "merge-base", "--is-ancestor", commit, "HEAD"],
            capture_output=True,
        )
        return result.returncode == 0

    problems = validate(data, is_ancestor)
    for problem in problems:
        print(f"CLOSED WP: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"closed work packages OK ({len(data)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
