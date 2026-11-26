#!/usr/bin/env python3
"""Validate the format of .deslop-please.allow (Python only).

Each non-comment line must be ``<rule> <key>`` with a known lint rule, a
non-empty key and no duplicates. No cargo and no SCIP indexing.

Usage:
    python3 tools/check_deslop_allowlist.py [--allow .deslop-please.allow]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

DEFAULT_ALLOW = Path(".deslop-please.allow")
RULES = {"module-cycle", "tangle", "field-leak", "shallow-module", "wide-directory", "long-file"}
ENTRY = re.compile(r"^([a-z][a-z-]*) ([^\s].*)$")


def validate(text: str) -> list[str]:
    """Return format problems in the allowlist."""
    problems: list[str] = []
    entries: dict[str, int] = {}
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = ENTRY.match(line)
        if not match:
            problems.append(f"line {number}: expected '<rule> <key>', got {line!r}")
            continue
        rule, key = match.groups()
        if rule not in RULES:
            problems.append(f"line {number}: unknown rule {rule!r}")
        if not key.strip():
            problems.append(f"line {number}: empty key")
        if line in entries:
            problems.append(f"line {number}: duplicate of line {entries[line]}")
        else:
            entries[line] = number
    if not entries:
        problems.append("no allowlist entries found")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow", type=Path, default=DEFAULT_ALLOW)
    args = parser.parse_args(argv)
    try:
        text = args.allow.read_text(encoding="utf-8")
    except OSError as error:
        print(f"check_deslop_allowlist: {error}", file=sys.stderr)
        return 1
    problems = validate(text)
    for problem in problems:
        print(f"DESLOP ALLOW: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"deslop allowlist format OK ({sum(1 for line in text.splitlines() if line.strip() and not line.startswith('#'))} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
