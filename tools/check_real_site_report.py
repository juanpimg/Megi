#!/usr/bin/env python3
"""Check docs/status/real-sites.json against the manifest and PNGs (Python only).

Usage:
    python3 tools/check_real_site_report.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tomllib
from pathlib import Path

MANIFEST = Path("docs/status/real_sites.toml")
REPORT = Path("docs/status/real-sites.json")
STATUS_DIR = Path("docs/status")
ALLOWED_RESULTS = {"PASS", "DEGRADADO", "PÁGINA EN BLANCO", "ERROR"}
ALLOWED_MODES = {"snapshot", "live"}


def validate(root: Path) -> list[str]:
    manifest_path, report_path = root / MANIFEST, root / REPORT
    if not manifest_path.is_file() or not report_path.is_file():
        return [f"missing {MANIFEST} or {REPORT}"]
    with manifest_path.open("rb") as handle:
        sites = tomllib.load(handle).get("site", [])
    report = json.loads(report_path.read_text(encoding="utf-8"))
    expected_ids = {str(site.get("id")) for site in sites}
    results = report.get("results")
    if not isinstance(results, list):
        return ["real-sites.json: results must be a list"]
    problems: list[str] = []
    seen: set[str] = set()
    for index, result in enumerate(results):
        label = f"results[{index}]"
        site_id = str(result.get("id", ""))
        if site_id not in expected_ids:
            problems.append(f"{label}: unknown site {site_id!r}")
        if site_id in seen:
            problems.append(f"{label}: duplicate site {site_id}")
        seen.add(site_id)
        if result.get("result") not in ALLOWED_RESULTS:
            problems.append(f"{label}: invalid result {result.get('result')!r}")
        if result.get("mode") not in ALLOWED_MODES:
            problems.append(f"{label}: invalid mode {result.get('mode')!r}")
        time_ms = result.get("time_ms")
        if not isinstance(time_ms, (int, float)) or time_ms < 0:
            problems.append(f"{label}: time_ms must be >= 0")
        items = result.get("display_items")
        if not isinstance(items, int) or items < 0:
            problems.append(f"{label}: display_items must be a non-negative int")
        png = str(result.get("png", ""))
        png_path = root / STATUS_DIR / png
        if not png or not png_path.is_file():
            problems.append(f"{label}: missing capture {png!r}")
            continue
        expected_md5 = result.get("png_md5")
        if expected_md5:
            actual = hashlib.md5(png_path.read_bytes()).hexdigest()
            if actual != expected_md5:
                problems.append(f"{label}: png_md5 mismatch ({actual} != {expected_md5})")
    for site_id in sorted(expected_ids - seen):
        problems.append(f"{site_id}: missing from real-sites.json")
    if report.get("mode") not in ALLOWED_MODES:
        problems.append(f"real-sites.json: invalid global mode {report.get('mode')!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"REAL SITE REPORT: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("real-site report consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
