#!/usr/bin/env python3
"""Check docs/status/test262.json against progress.toml [test262] (Python only).

Usage:
    python3 tools/check_test262_artifacts.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path

REPORT = Path("docs/status/test262.json")
MANIFEST = Path("docs/status/progress.toml")


def validate(root: Path) -> list[str]:
    report_path, manifest_path = root / REPORT, root / MANIFEST
    if not report_path.is_file() or not manifest_path.is_file():
        return [f"missing {REPORT} or {MANIFEST}"]
    report = json.loads(report_path.read_text(encoding="utf-8"))
    with manifest_path.open("rb") as handle:
        block = tomllib.load(handle).get("test262", {})
    problems: list[str] = []
    if report.get("schema") != "megi-test262-report/1":
        problems.append(f"test262.json: unexpected schema {report.get('schema')!r}")
    epoch = report.get("epoch", {})
    for key in ("manifest_sha256", "scope"):
        if epoch.get(key) != block.get(key):
            problems.append(f"test262.json: epoch.{key} disagrees with progress.toml")
    passed = report.get("counts", {}).get("pass")
    total = report.get("total")
    if passed != block.get("passed") or total != block.get("total"):
        problems.append(
            f"test262.json: {passed}/{total} disagrees with progress.toml "
            f"{block.get('passed')}/{block.get('total')}"
        )
    if sum(v for v in report.get("counts", {}).values() if isinstance(v, int)) != total:
        problems.append("test262.json: counts do not add up to total")
    if report.get("allowlist") != block.get("allowlist"):
        problems.append("test262.json: allowlist size disagrees with progress.toml")
    headline = f"{passed}/{total}"
    if headline not in str(report.get("headline", "")):
        problems.append(f"test262.json: headline is missing {headline}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"TEST262 ARTIFACT: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("Test262 artifacts consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
