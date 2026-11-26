#!/usr/bin/env python3
"""Check SOURCES and GENERATORS in tools/wp_generate.py agree (Python only).

Usage:
    python3 tools/check_wp_generator_functions.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

GENERATOR = Path("tools/wp_generate.py")
SOURCES = re.compile(r"SOURCES\s*=\s*\(([^)]*)\)", re.S)
GENERATORS = re.compile(r"GENERATORS\s*=\s*\{([^}]*)\}", re.S)
NAME = re.compile(r"['\"]([a-z0-9_-]+)['\"]")


def _tuple_names(text: str, pattern: re.Pattern[str]) -> set[str]:
    match = pattern.search(text)
    return set(NAME.findall(match.group(1))) if match else set()


def validate(root: Path) -> list[str]:
    path = root / GENERATOR
    if not path.is_file():
        return [f"missing {GENERATOR}"]
    text = path.read_text(encoding="utf-8")
    sources = _tuple_names(text, SOURCES)
    generators = _tuple_names(text, GENERATORS)
    problems: list[str] = []
    if not sources:
        problems.append("wp_generate.py: SOURCES not found")
    if not generators:
        problems.append("wp_generate.py: GENERATORS not found")
    for source in sorted(sources - generators):
        problems.append(f"{source}: SOURCES entry has no GENERATORS function")
    for source in sorted(generators - sources):
        problems.append(f"{source}: GENERATORS function has no SOURCES entry")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WP GENERATOR FUNCTIONS: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("work package generator functions consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
