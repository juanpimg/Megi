#!/usr/bin/env python3
"""Check docs/status/wpt.md against docs/status/wpt.json (Python only).

Usage:
    python3 tools/check_wpt_report_md.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

JSON_PATH = Path("docs/status/wpt.json")
MD_PATH = Path("docs/status/wpt.md")
LABELS = {"pass": "PASS", "fail": "FAIL", "timeout": "TIMEOUT", "notrun": "NOTRUN", "error": "harness_error"}


def validate(root: Path) -> list[str]:
    json_path, md_path = root / JSON_PATH, root / MD_PATH
    if not json_path.is_file() or not md_path.is_file():
        return [f"missing {JSON_PATH} or {MD_PATH}"]
    report = json.loads(json_path.read_text(encoding="utf-8"))
    text = md_path.read_text(encoding="utf-8")
    problems: list[str] = []
    headline = f"{report.get('total')}/{report.get('total')}"
    if headline not in text:
        problems.append(f"wpt.md: missing headline {headline}")
    for key, label in LABELS.items():
        value = report.get("counts", {}).get(key)
        if not re.search(rf"^\| {re.escape(label)} \| {value} \|$", text, re.M):
            problems.append(f"wpt.md: missing row {label}={value}")
    epoch = report.get("epoch", {})
    for key in ("wpt_revision", "manifest_sha256"):
        if epoch.get(key) not in text:
            problems.append(f"wpt.md: missing epoch {key}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WPT REPORT MD: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("WPT report markdown consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
