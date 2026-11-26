#!/usr/bin/env python3
"""Check that docs/VISUAL-PARITY.md matches the versioned parity baseline.

The Markdown table has three metric columns; this checker validates the middle
one ("Tras system-ui + pistas legacy"), which is the state recorded in
``tests/fixtures/visual-parity/baseline.json``. Python only, no cargo.

Usage:
    python3 tools/check_parity_docs.py [--doc docs/VISUAL-PARITY.md]
                                       [--baseline tests/fixtures/visual-parity/baseline.json]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

DEFAULT_DOC = Path("docs/VISUAL-PARITY.md")
DEFAULT_BASELINE = Path("tests/fixtures/visual-parity/baseline.json")
SITE_ALIASES = {
    "example.com": "example",
    "rust-lang.org": "rust",
    "news.ycombinator.com": "hn",
    "wikipedia": "wikipedia",
}
METRIC = re.compile(r"([\d.,]+)\s*%\s*/\s*([\d.,]+)")


def _number(raw: str) -> float:
    value = raw.replace(" ", "")
    if "," in value and "." in value:  # European separator: 1.234,56
        value = value.replace(".", "").replace(",", ".")
    elif "," in value:  # 0,92
        value = value.replace(",", ".")
    return float(value)


def parse_table(text: str) -> dict[str, tuple[float, float]]:
    """Extract {site: (changed_pct, mean_abs_diff)} from the second metric column."""
    result: dict[str, tuple[float, float]] = {}
    for line in text.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) < 3:
            continue
        label = cells[0].replace("**", "").strip()
        key = label.lower()
        if key not in SITE_ALIASES:
            continue
        match = METRIC.search(cells[2].replace("**", ""))
        if not match:
            raise ValueError(f"{label}: malformed metrics cell {cells[2]!r}")
        result[SITE_ALIASES[key]] = (_number(match.group(1)), _number(match.group(2)))
    return result


def validate(doc_text: str, baseline: dict, tolerance: float = 0.01) -> list[str]:
    """Compare the documented table with the baseline; [] means they agree."""
    problems: list[str] = []
    try:
        documented = parse_table(doc_text)
    except ValueError as error:
        return [str(error)]
    for site, metrics in baseline.items():
        expected = (float(metrics["changed_pct"]), float(metrics["mean_abs_diff"]))
        actual = documented.get(site)
        if actual is None:
            problems.append(f"{site}: missing row in {DEFAULT_DOC}")
            continue
        for index, key in enumerate(("changed_pct", "mean_abs_diff")):
            if abs(actual[index] - expected[index]) > tolerance:
                problems.append(
                    f"{site}: docs {key}={actual[index]} but baseline={expected[index]}"
                )
    for site in documented:
        if site not in baseline:
            problems.append(f"{site}: table row has no baseline entry")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    args = parser.parse_args(argv)
    try:
        doc_text = args.doc.read_text(encoding="utf-8")
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"check_parity_docs: {error}", file=sys.stderr)
        return 1
    problems = validate(doc_text, baseline)
    for problem in problems:
        print(f"MISMATCH: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"parity docs match baseline ({len(baseline)} sites)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
