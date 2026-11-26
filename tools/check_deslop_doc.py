#!/usr/bin/env python3
"""Check that docs/DESLOP-AUDIT.md matches .deslop-please.allow (Python only).

The document records the original SCIP baseline per rule and a verified delta;
this checker verifies those figures against the current allowlist inventory.

Usage:
    python3 tools/check_deslop_doc.py [--doc docs/DESLOP-AUDIT.md]
                                      [--allow .deslop-please.allow]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

DEFAULT_DOC = Path("docs/DESLOP-AUDIT.md")
DEFAULT_ALLOW = Path(".deslop-please.allow")
RULES = ("field-leak", "module-cycle", "tangle", "shallow-module", "wide-directory", "long-file")


def parse_baseline_table(text: str) -> dict[str, int]:
    """Read the ``| Regla | Hallazgos |`` table in the document."""
    counts: dict[str, int] = {}
    in_table = False
    for line in text.splitlines():
        if line.startswith("| Regla |"):
            in_table = True
            continue
        if in_table and not line.startswith("|"):
            break
        if not in_table or not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) != 2 or cells[0] in ("Regla", "---"):
            continue
        rule = cells[0].strip("` ")
        if rule in RULES:
            counts[rule] = int(cells[1])
    return counts


def parse_claim(text: str) -> tuple[int, int, int, int]:
    """Return (baseline total, current total, removed, added) from the prose."""
    baseline = re.search(r"produjo \*\*(\d+) hallazgos\*\*", text)
    delta = re.search(r"pasó de \*\*(\d+) a (\d+) entradas\*\*", text)
    removed = re.search(r"se eliminaron (\d+)", text)
    added = re.search(r"se añadieron (\d+)", text)
    if not all((baseline, delta, removed, added)):
        raise ValueError("the document is missing the baseline/delta figures")
    return (
        int(baseline.group(1)),
        int(delta.group(2)),
        int(removed.group(1)),
        int(added.group(1)),
    )


def count_allowlist(text: str) -> dict[str, int]:
    """Count allowlist entries by rule, ignoring comments and blank lines."""
    counts: dict[str, int] = {}
    for line in text.splitlines():
        entry = line.strip()
        if not entry or entry.startswith("#"):
            continue
        rule = entry.split()[0]
        counts[rule] = counts.get(rule, 0) + 1
    return counts


def validate(doc_text: str, allow_text: str) -> list[str]:
    """Return inconsistencies between the document and the allowlist."""
    problems: list[str] = []
    try:
        baseline_total, current_total, removed, added = parse_claim(doc_text)
    except ValueError as error:
        return [str(error)]
    table = parse_baseline_table(doc_text)
    for rule in RULES:
        if rule not in table:
            problems.append(f"baseline table has no row for {rule}")
    if sum(table.values()) != baseline_total:
        problems.append(
            f"baseline table sums to {sum(table.values())}, document claims {baseline_total}"
        )
    if baseline_total - removed + added != current_total:
        problems.append(
            f"delta inconsistent: {baseline_total} - {removed} + {added} != {current_total}"
        )
    counts = count_allowlist(allow_text)
    if sum(counts.values()) != current_total:
        problems.append(
            f"allowlist has {sum(counts.values())} entries, document claims {current_total}"
        )
    for rule in counts:
        if rule not in RULES:
            problems.append(f"allowlist has unknown rule {rule!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--allow", type=Path, default=DEFAULT_ALLOW)
    args = parser.parse_args(argv)
    try:
        doc_text = args.doc.read_text(encoding="utf-8")
        allow_text = args.allow.read_text(encoding="utf-8")
    except OSError as error:
        print(f"check_deslop_doc: {error}", file=sys.stderr)
        return 1
    problems = validate(doc_text, allow_text)
    for problem in problems:
        print(f"MISMATCH: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"deslop doc matches allowlist ({sum(count_allowlist(allow_text).values())} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
