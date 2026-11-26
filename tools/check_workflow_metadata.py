#!/usr/bin/env python3
"""Check basic GitHub workflow structure with Python stdlib only.

Verifies YAML files under .github/workflows have name/on/jobs, no tab
indentation and a unique job id set. No cargo and no YAML dependency.

Usage:
    python3 tools/check_workflow_metadata.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

WORKFLOWS = Path(".github/workflows")
JOB = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$", re.M)


def check_file(text: str) -> list[str]:
    """Return structural problems for one workflow document."""
    problems: list[str] = []
    if "\t" in text:
        problems.append("tab indentation is not allowed in YAML")
    if not re.search(r"^name:\s*\S", text, re.M):
        problems.append("missing top-level name:")
    if not re.search(r"^on:\s*", text, re.M):
        problems.append("missing top-level on:")
    jobs = JOB.findall(text)
    if not jobs:
        problems.append("no jobs declared")
    if len(jobs) != len(set(jobs)):
        problems.append("duplicate job ids")
    return problems


def validate(root: Path) -> list[str]:
    directory = root / WORKFLOWS
    if not directory.is_dir():
        return [f"missing {WORKFLOWS}"]
    files = sorted(p for p in directory.iterdir() if p.suffix in (".yml", ".yaml") and p.is_file())
    if not files:
        return ["no workflows found"]
    problems: list[str] = []
    for path in files:
        for problem in check_file(path.read_text(encoding="utf-8")):
            problems.append(f"{path.name}: {problem}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WORKFLOW: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("workflow metadata OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
