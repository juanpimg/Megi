#!/usr/bin/env python3
"""Check docs/status/real-sites.html against real-sites.json (Python only).

Usage:
    python3 tools/check_real_site_html.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

JSON_PATH = Path("docs/status/real-sites.json")
HTML_PATH = Path("docs/status/real-sites.html")
SUMMARY = re.compile(
    r"Modo: <b>(\w+)</b>.*?PASS: (\d+) · DEGRADADO: (\d+) · "
    r"PÁGINA EN BLANCO: (\d+) · ERROR: (\d+)"
)
ROW = re.compile(r"<tr><td><b>(.*?)</b>.*?<td[^>]*><b>(.*?)</b></td>", re.S)


def validate(root: Path) -> list[str]:
    json_path, html_path = root / JSON_PATH, root / HTML_PATH
    if not json_path.is_file() or not html_path.is_file():
        return [f"missing {JSON_PATH} or {HTML_PATH}"]
    report = json.loads(json_path.read_text(encoding="utf-8"))
    html = html_path.read_text(encoding="utf-8")
    problems: list[str] = []
    summary = SUMMARY.search(html)
    if not summary:
        problems.append("real-sites.html: summary line is missing or malformed")
    else:
        mode, *counts = summary.groups()
        if mode != report.get("mode"):
            problems.append(f"real-sites.html: mode {mode!r} != json {report.get('mode')!r}")
        expected = Counter(item.get("result") for item in report.get("results", []))
        for label, value in zip(("PASS", "DEGRADADO", "PÁGINA EN BLANCO", "ERROR"), counts):
            if int(value) != expected.get(label, 0):
                problems.append(
                    f"real-sites.html: {label}={value} but json has {expected.get(label, 0)}"
                )
    rows = dict(ROW.findall(html))
    for item in report.get("results", []):
        name, result = item.get("name"), item.get("result")
        if name not in rows:
            problems.append(f"real-sites.html: no row for {name!r}")
        elif rows[name] != result:
            problems.append(f"real-sites.html: {name} shows {rows[name]!r}, json says {result!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"REAL SITE HTML: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("real-site HTML consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
