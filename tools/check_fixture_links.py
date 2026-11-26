#!/usr/bin/env python3
"""Check fixture symlinks resolve inside the repository (Python only).

Usage:
    python3 tools/check_fixture_links.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

FIXTURES = Path("tests/fixtures")


def validate(root: Path) -> list[str]:
    directory = root / FIXTURES
    if not directory.is_dir():
        return [f"missing {FIXTURES}"]
    problems: list[str] = []
    count = 0
    for link in sorted(directory.rglob("*")):
        if not link.is_symlink():
            continue
        count += 1
        target = Path(link.readlink())
        resolved = (link.parent / target).resolve()
        if not resolved.is_relative_to(root.resolve()):
            problems.append(f"{link.relative_to(root)}: symlink escapes the repository")
        if not resolved.exists():
            problems.append(f"{link.relative_to(root)}: broken symlink -> {target}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"FIXTURE LINK: {problem}", file=sys.stderr)
    if problems:
        return 1
    count = sum(1 for link in (args.root.resolve() / FIXTURES).rglob("*") if link.is_symlink())
    print(f"fixture links OK ({count} symlinks)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
