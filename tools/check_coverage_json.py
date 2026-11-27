#!/usr/bin/env python3
"""Check docs/status/coverage.json against docs/status/coverage.toml (Python only).

Usage:
    python3 tools/check_coverage_json.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path

MANIFEST = Path("docs/status/coverage.toml")
REPORT = Path("docs/status/coverage.json")


def expected_percent(implemented: int, known: int) -> int:
    if known <= 0:
        return 0
    if implemented >= known:
        return 100
    return min(99, round(implemented * 100 / known))


def validate(root: Path) -> list[str]:
    manifest_path, report_path = root / MANIFEST, root / REPORT
    if not manifest_path.is_file() or not report_path.is_file():
        return [f"missing {MANIFEST} or {REPORT}"]
    with manifest_path.open("rb") as handle:
        capabilities = tomllib.load(handle).get("capability", [])
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if not capabilities:
        return ["coverage.toml has no capabilities"]
    done = sum(1 for cap in capabilities if cap.get("state") == "done")
    partial = sum(1 for cap in capabilities if cap.get("state") == "partial")
    known = len(capabilities)
    overall = report.get("overall", {})
    problems: list[str] = []
    for key, expected in (
        ("implemented", done),
        ("partial", partial),
        ("known", known),
        ("percent", expected_percent(done, known)),
    ):
        if overall.get(key) != expected:
            problems.append(f"coverage.json overall.{key}={overall.get(key)!r}, expected {expected}")
    areas = report.get("areas", [])
    if not isinstance(areas, list) or not areas:
        problems.append("coverage.json has no areas")
    else:
        bad_counters = [
            f"areas[{index}].{key}={area.get(key)!r}"
            for index, area in enumerate(areas)
            for key in ("implemented", "known")
            if type(area.get(key, 0)) is not int
        ]
        if bad_counters:
            problems.append(
                "coverage.json area counters must be integers: " + ", ".join(bad_counters)
            )
        implemented = sum(
            area.get("implemented", 0) for area in areas
            if type(area.get("implemented", 0)) is int
        )
        counted = sum(
            area.get("known", 0) for area in areas
            if type(area.get("known", 0)) is int
        )
        if (implemented, counted) != (done, known):
            problems.append(
                f"coverage.json areas sum to {implemented}/{counted}, expected {done}/{known}"
            )
    if report.get("schema") != 1:
        problems.append(f"coverage.json unexpected schema {report.get('schema')!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"COVERAGE JSON: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("coverage JSON consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
