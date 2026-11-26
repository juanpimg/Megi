#!/usr/bin/env python3
"""Check md5 annotations in real-sites.html against real-sites.json (Python only).

Usage:
    python3 tools/check_real_site_md5.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

JSON_PATH = Path("docs/status/real-sites.json")
HTML_PATH = Path("docs/status/real-sites.html")
ROW = re.compile(r"<tr>.*?</tr>", re.S)
ALT = re.compile(r"alt='captura de (.*?)'")
MD5 = re.compile(r"md5 <code title='md5 completo'>([0-9a-f]{32})</code>")


def validate(root: Path) -> list[str]:
    json_path, html_path = root / JSON_PATH, root / HTML_PATH
    if not json_path.is_file() or not html_path.is_file():
        return [f"missing {JSON_PATH} or {HTML_PATH}"]
    report = json.loads(json_path.read_text(encoding="utf-8"))
    html = html_path.read_text(encoding="utf-8")
    problems: list[str] = []
    for row in ROW.findall(html):
        name_match = re.match(r"<tr><td><b>(.*?)</b>", row, re.S)
        if not name_match:
            continue
        name = name_match.group(1)
        alt_match = ALT.search(row)
        alt = alt_match.group(1) if alt_match else None
        md5_match = MD5.search(row)
        md5 = md5_match.group(1) if md5_match else None
        if alt and name != alt:
            problems.append(f"real-sites.html: row name {name!r} != image alt {alt!r}")
        item = next((entry for entry in report.get("results", []) if entry.get("name") == name), None)
        if item is None:
            continue
        expected = item.get("png_md5")
        if expected and md5 and md5 != expected:
            problems.append(f"{name}: html md5 {md5} != json {expected}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"REAL SITE MD5: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("real-site md5 annotations consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
