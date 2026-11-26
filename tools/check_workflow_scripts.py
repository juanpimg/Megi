#!/usr/bin/env python3
"""Check tools/* scripts referenced by GitHub workflows exist and are audited.

Python only: regular expressions over the workflow YAML plus the audit ledger.

Usage:
    python3 tools/check_workflow_scripts.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

WORKFLOWS = Path(".github/workflows")
LEDGER = Path("docs/audit/coverage.tsv")
REFERENCE = re.compile(r"tools/[A-Za-z0-9_./-]+\.(?:py|sh)")


def extract_refs(text: str) -> set[str]:
    return set(REFERENCE.findall(text))


def read_ledger(path: Path) -> dict[str, dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return {row["path"]: row for row in csv.DictReader(handle, delimiter="\t")}


def validate(root: Path) -> list[str]:
    workflows = root / WORKFLOWS
    if not workflows.is_dir():
        return [f"missing {WORKFLOWS}"]
    refs: set[str] = set()
    for path in sorted(workflows.iterdir()):
        if path.is_file() and path.suffix in (".yml", ".yaml"):
            refs.update(extract_refs(path.read_text(encoding="utf-8")))
    if not refs:
        return ["no tools/* scripts referenced by workflows"]
    ledger = read_ledger(root / LEDGER)
    problems: list[str] = []
    for relative in sorted(refs):
        if not (root / relative).is_file():
            problems.append(f"{relative}: referenced by a workflow but missing")
            continue
        row = ledger.get(relative)
        if row is None:
            problems.append(f"{relative}: missing from audit ledger")
        elif row.get("status") != "covered":
            problems.append(f"{relative}: ledger status is {row.get('status')!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WORKFLOW SCRIPT: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("workflow script references OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
