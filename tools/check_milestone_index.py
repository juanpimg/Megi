#!/usr/bin/env python3
"""Check AGENT-INDEX.md milestone table against progress.toml phases (Python only).

Usage:
    python3 tools/check_milestone_index.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

INDEX = Path("docs/AGENT-INDEX.md")
PROGRESS = Path("docs/status/progress.toml")
MILESTONE = re.compile(r"^\|\s*(M\d+)\s*\|", re.M)


def validate(root: Path) -> list[str]:
    index_path, progress_path = root / INDEX, root / PROGRESS
    if not index_path.is_file() or not progress_path.is_file():
        return [f"missing {INDEX} or {PROGRESS}"]
    listed = MILESTONE.findall(index_path.read_text(encoding="utf-8"))
    with progress_path.open("rb") as handle:
        phases = [str(p.get("id", "")) for p in tomllib.load(handle).get("phase", [])]
    problems: list[str] = []
    if len(listed) != len(set(listed)):
        problems.append("AGENT-INDEX.md: duplicate milestone rows")
    if len(phases) != len(set(phases)):
        problems.append("progress.toml: duplicate [[phase]] ids")
    for milestone in sorted(set(listed) - set(phases)):
        problems.append(f"{milestone}: in AGENT-INDEX.md but not in progress.toml")
    for milestone in sorted(set(phases) - set(listed)):
        problems.append(f"{milestone}: in progress.toml but not in AGENT-INDEX.md")
    if not listed or not phases:
        problems.append("no milestones found")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"MILESTONE INDEX: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("milestone index consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
