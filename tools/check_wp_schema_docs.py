#!/usr/bin/env python3
"""Check docs/work-packages/SCHEMA.md against tools/wp_schema.py (Python only).

Usage:
    python3 tools/check_wp_schema_docs.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SCHEMA = Path("docs/work-packages/SCHEMA.md")
VALIDATOR = Path("tools/wp_schema.py")
FIELD = re.compile(r"^\|\s*`([a-z_]+)`\s*\|", re.M)
REQUIRED = re.compile(r"REQUIRED_FIELDS\s*=\s*\(([^)]*)\)", re.S)
NAME = re.compile(r'"([a-z_]+)"')


def documented_fields(text: str) -> set[str]:
    head = text.split("## Campos obligatorios", 1)[-1].split("## ", 1)[0]
    return set(FIELD.findall(head))


def implemented_fields(text: str) -> set[str]:
    match = REQUIRED.search(text)
    if not match:
        return set()
    return set(NAME.findall(match.group(1)))


def validate(root: Path) -> list[str]:
    schema_path, validator_path = root / SCHEMA, root / VALIDATOR
    if not schema_path.is_file() or not validator_path.is_file():
        return [f"missing {SCHEMA} or {VALIDATOR}"]
    documented = documented_fields(schema_path.read_text(encoding="utf-8"))
    implemented = implemented_fields(validator_path.read_text(encoding="utf-8"))
    problems: list[str] = []
    if not documented:
        problems.append("SCHEMA.md: no required fields found")
    if not implemented:
        problems.append("wp_schema.py: REQUIRED_FIELDS not found")
    for field in sorted(documented - implemented):
        problems.append(f"{field}: documented but not enforced by wp_schema.py")
    for field in sorted(implemented - documented):
        problems.append(f"{field}: enforced by wp_schema.py but undocumented")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WP SCHEMA: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("work package schema docs consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
