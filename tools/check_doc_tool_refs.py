#!/usr/bin/env python3
"""Check tool references in docs exist, have tests and are in the audit ledger.

Python only. Scans ``AGENTS.md`` and ``docs/VISUAL-PARITY.md`` for
``tools/<name>.py`` references and verifies each one.

Usage:
    python3 tools/check_doc_tool_refs.py [--docs AGENTS.md docs/VISUAL-PARITY.md]
                                        [--ledger docs/audit/coverage.tsv]
"""
from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

DEFAULT_DOCS = (
    "AGENTS.md",
    "CONTRIBUTING.md",
    "docs/DESLOP-AUDIT.md",
    "docs/VISUAL-PARITY.md",
)
DEFAULT_LEDGER = Path("docs/audit/coverage.tsv")
REFERENCE = re.compile(r"tools/[A-Za-z0-9_./-]+\.py")


def extract_refs(text: str) -> set[str]:
    """Return the tool paths referenced by a Markdown document."""
    return set(REFERENCE.findall(text))


def count_lines(path: Path) -> int:
    data = path.read_bytes()
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def read_ledger(path: Path) -> dict[str, dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return {row["path"]: row for row in csv.DictReader(handle, delimiter="\t")}


def validate(root: Path, docs: tuple[str, ...], ledger_path: Path) -> list[str]:
    """Return problems for referenced tools lacking a file, test or ledger row."""
    problems: list[str] = []
    try:
        ledger = read_ledger(ledger_path)
    except OSError as error:
        return [f"cannot read ledger: {error}"]
    for doc in docs:
        path = root / doc
        try:
            refs = extract_refs(path.read_text(encoding="utf-8"))
        except OSError as error:
            problems.append(f"{doc}: cannot read ({error})")
            continue
        if not refs:
            problems.append(f"{doc}: no tool references found")
        for relative in sorted(refs):
            target = root / relative
            if not target.is_file():
                problems.append(f"{doc}: {relative} does not exist")
                continue
            test = f"tools/test_{Path(relative).stem}.py"
            if not (root / test).is_file():
                problems.append(f"{doc}: {relative} has no {test}")
            row = ledger.get(relative)
            if row is None:
                problems.append(f"{doc}: {relative} missing from ledger")
                continue
            if row.get("status") != "covered":
                problems.append(f"{doc}: {relative} is {row.get('status')!r} in ledger")
            if row.get("lines") != str(count_lines(target)):
                problems.append(f"{doc}: {relative} has a stale line count in ledger")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docs", nargs="*", default=list(DEFAULT_DOCS))
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    ledger = args.ledger if args.ledger.is_absolute() else root / args.ledger
    problems = validate(root, tuple(args.docs), ledger)
    for problem in problems:
        print(f"TOOL REF: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"documented tools OK ({len(args.docs)} docs)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
