#!/usr/bin/env python3
"""Check that AGENTS.md limits match the constants enforced by the tools.

Python only; parses text with regex, no cargo.

Usage:
    python3 tools/check_agents_constraints.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

AGENTS = Path("AGENTS.md")
ARCH = Path("tools/check_arch.py")
PLAN = Path("tools/wp_plan.py")


def int_const(text: str, name: str) -> int | None:
    match = re.search(rf"^{name}\s*=\s*(\d+)\s*$", text, re.M)
    return int(match.group(1)) if match else None


def validate(root: Path) -> list[str]:
    agents = root / AGENTS
    arch = root / ARCH
    plan = root / PLAN
    if not all(path.is_file() for path in (agents, arch, plan)):
        return ["missing AGENTS.md, tools/check_arch.py or tools/wp_plan.py"]
    rules = agents.read_text(encoding="utf-8")
    problems: list[str] = []
    if not re.search(r"\b500\b", rules):
        problems.append("AGENTS.md: the 500-line hard limit is not documented")
    if not re.search(r"\b10\b", rules):
        problems.append("AGENTS.md: the 10 in_progress cap is not documented")
    if "forbid(unsafe_code)" not in rules:
        problems.append("AGENTS.md: forbid(unsafe_code) veto is missing")
    max_lines = int_const(arch.read_text(encoding="utf-8"), "MAX_LINES")
    if max_lines != 500:
        problems.append(f"check_arch.py MAX_LINES={max_lines}, AGENTS.md says 500")
    max_progress = int_const(plan.read_text(encoding="utf-8"), "MAX_IN_PROGRESS")
    if max_progress != 10:
        problems.append(f"wp_plan.py MAX_IN_PROGRESS={max_progress}, AGENTS.md says 10")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"AGENTS CONSTRAINTS: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("agent constraints consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
