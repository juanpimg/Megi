#!/usr/bin/env python3
"""Check the per-area table in coverage.html against coverage.json/toml.

Python only; no cargo.

Usage:
    python3 tools/check_coverage_html_areas.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

JSON_PATH = Path("docs/status/coverage.json")
HTML_PATH = Path("docs/status/coverage.html")
TOML_PATH = Path("docs/status/coverage.toml")
AREA_ROW = re.compile(
    r"<td><b>([A-Z0-9]+)</b>.*?</td><td>(\d+)</td><td>(\d+)</td>", re.S
)


def validate(root: Path) -> list[str]:
    paths = [root / p for p in (JSON_PATH, HTML_PATH, TOML_PATH)]
    if not all(path.is_file() for path in paths):
        return ["missing coverage.json, coverage.html or coverage.toml"]
    report = json.loads((root / JSON_PATH).read_text(encoding="utf-8"))
    html = (root / HTML_PATH).read_text(encoding="utf-8")
    with (root / TOML_PATH).open("rb") as handle:
        capabilities = tomllib.load(handle).get("capability", [])
    shown = {area: (int(done), int(total)) for area, done, total in AREA_ROW.findall(html)}
    if not shown:
        return ["coverage.html: no per-area rows found"]
    problems: list[str] = []
    for area in report.get("areas", []):
        expected = (area.get("implemented"), area.get("known"))
        if shown.get(area.get("id")) != expected:
            problems.append(
                f"coverage.html: area {area.get('id')} shows {shown.get(area.get('id'))}, expected {expected}"
            )
    missing_section = html.split("<h2>Aún faltan", 1)[-1]
    for cap in capabilities:
        if cap.get("state") != "done" and str(cap.get("id")) not in missing_section:
            problems.append(f"coverage.html: unfinished capability {cap.get('id')} is not listed")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"COVERAGE HTML: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("coverage HTML areas consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
