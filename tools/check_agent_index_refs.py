#!/usr/bin/env python3
"""Check that document/plan paths referenced by AGENT-INDEX.md exist (Python only).

Usage:
    python3 tools/check_agent_index_refs.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

INDEX = Path("docs/AGENT-INDEX.md")
PLANS = Path("docs/superpowers/plans")
BACKTICK = re.compile(r"`([^`]+)`")
EXTENSIONS = (".md", ".toml", ".json", ".html", ".py", ".sh", ".jpg", ".png")
PLAN_NAME = re.compile(r"^\d{4}-\d{2}-\d{2}-.+\.md$")


def referenced_paths(text: str) -> set[str]:
    """Return repository-relative paths mentioned inside backticks."""
    result: set[str] = set()
    for raw in BACKTICK.findall(text):
        value = raw.strip()
        if "*" in value or not value.endswith(EXTENSIONS):
            continue
        if "/" in value:
            result.add(value)
        elif PLAN_NAME.match(value):
            result.add((PLANS / value).as_posix())
        else:
            result.add(value)  # e.g. PROGRESS.md at the repository root
    return result


def validate(root: Path) -> list[str]:
    index = root / INDEX
    if not index.is_file():
        return [f"missing {INDEX}"]
    problems: list[str] = []
    paths = referenced_paths(index.read_text(encoding="utf-8"))
    if not paths:
        return ["no document paths found in AGENT-INDEX.md"]
    for relative in sorted(paths):
        if not (root / relative).exists():
            problems.append(f"{relative}: referenced in AGENT-INDEX.md but missing")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"AGENT INDEX: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"agent index references OK ({len(referenced_paths((args.root.resolve() / INDEX).read_text(encoding='utf-8')))} paths)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
