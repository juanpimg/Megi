#!/usr/bin/env python3
"""Check docs/status/coverage.svg against coverage.json (Python only).

Usage:
    python3 tools/check_coverage_svg.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

JSON_PATH = Path("docs/status/coverage.json")
SVG_PATH = Path("docs/status/coverage.svg")


def validate(root: Path) -> list[str]:
    json_path, svg_path = root / JSON_PATH, root / SVG_PATH
    if not json_path.is_file() or not svg_path.is_file():
        return [f"missing {JSON_PATH} or {SVG_PATH}"]
    report = json.loads(json_path.read_text(encoding="utf-8"))
    svg = svg_path.read_text(encoding="utf-8")
    overall = report.get("overall", {})
    implemented, known = overall.get("implemented"), overall.get("known")
    percent = overall.get("percent")
    problems: list[str] = []
    headline = f"{implemented}/{known}"
    if headline not in svg:
        problems.append(f"coverage.svg: missing overall headline {headline}")
    if f"{percent}%" not in svg:
        problems.append(f"coverage.svg: missing percent {percent}%")
    for area in report.get("areas", []):
        label = f"{area.get('implemented')}/{area.get('known')}"
        if label not in svg:
            problems.append(f"coverage.svg: missing area count {label} ({area.get('id')})")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"COVERAGE SVG: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("coverage SVG consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
