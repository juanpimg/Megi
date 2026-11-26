#!/usr/bin/env python3
"""Check SCHEMA.md source/generator table against tools/wp_generate.py (Python only).

Usage:
    python3 tools/check_wp_generators.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SCHEMA = Path("docs/work-packages/SCHEMA.md")
GENERATOR = Path("tools/wp_generate.py")
ROW_GENERATOR = re.compile(r"(?:wp_generate\.py|\.\.\.)\s+([a-z0-9_-]+)")
SOURCES = re.compile(r"SOURCES\s*=\s*\(([^)]*)\)")
NAME = re.compile(r'"([a-z0-9_-]+)"')


def documented_sources(text: str) -> set[str]:
    section = text.split("## Fuentes y generadores", 1)[-1].split("## ", 1)[0]
    return set(ROW_GENERATOR.findall(section))


def implemented_sources(text: str) -> set[str]:
    match = SOURCES.search(text)
    return set(NAME.findall(match.group(1))) if match else set()


def validate(root: Path) -> list[str]:
    schema_path, generator_path = root / SCHEMA, root / GENERATOR
    if not schema_path.is_file() or not generator_path.is_file():
        return [f"missing {SCHEMA} or {GENERATOR}"]
    documented = documented_sources(schema_path.read_text(encoding="utf-8"))
    implemented = implemented_sources(generator_path.read_text(encoding="utf-8"))
    problems: list[str] = []
    if not documented:
        problems.append("SCHEMA.md: no generators documented")
    if not implemented:
        problems.append("wp_generate.py: SOURCES not found")
    for source in sorted(documented - implemented):
        problems.append(f"{source}: documented generator has no SOURCES entry")
    for source in sorted(implemented - documented):
        problems.append(f"{source}: SOURCES entry has no documented generator")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WP GENERATORS: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("work package generators consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
