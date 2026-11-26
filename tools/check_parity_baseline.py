#!/usr/bin/env python3
"""Check a visual-parity report against the versioned baseline (Python only).

Usage:
    python3 tools/check_parity_baseline.py report.json
        [--baseline tests/fixtures/visual-parity/baseline.json]
        [--tol-pct 0.5] [--tol-mean 1.0]

A site regresses when its changed-pixel percentage or mean absolute RGB
difference grows beyond the tolerance. Missing sites are regressions; new sites
are reported as informational. Generate ``report.json`` with
``tools/visual_parity_report.py``.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

DEFAULT_BASELINE = Path("tests/fixtures/visual-parity/baseline.json")


def regressions(baseline: dict, current: dict, tol_pct: float, tol_mean: float) -> list[str]:
    """Return human-readable regressions; an empty list means parity is preserved."""
    problems: list[str] = []
    for site, expected in baseline.items():
        actual = current.get(site)
        if actual is None:
            problems.append(f"{site}: missing from the new report")
            continue
        for key, tolerance in (("changed_pct", tol_pct), ("mean_abs_diff", tol_mean)):
            before = float(expected[key])
            after = float(actual[key])
            if after > before + tolerance:
                problems.append(
                    f"{site}: {key} {before} -> {after} (tolerance +{tolerance})"
                )
        if expected.get("size") != actual.get("size"):
            problems.append(f"{site}: viewport {expected.get('size')} -> {actual.get('size')}")
    for site in current:
        if site not in baseline:
            print(f"note: new site {site} is not in the baseline", file=sys.stderr)
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--tol-pct", type=float, default=0.5)
    parser.add_argument("--tol-mean", type=float, default=1.0)
    args = parser.parse_args(argv)
    if args.tol_pct < 0 or args.tol_mean < 0:
        print("check_parity_baseline: tolerances must be >= 0", file=sys.stderr)
        return 2
    try:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        current = json.loads(args.report.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"check_parity_baseline: {error}", file=sys.stderr)
        return 1
    problems = regressions(baseline, current, args.tol_pct, args.tol_mean)
    for problem in problems:
        print(f"REGRESSION: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"parity baseline OK ({len(baseline)} sites)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
