#!/usr/bin/env python3
"""Check every vendored WPT fixture against tests/wpt/MANIFEST.json (Python only).

Usage:
    python3 tools/check_wpt_manifest_files.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

MANIFEST = Path("tests/wpt/MANIFEST.json")
ROOT = Path("tests/wpt")


def validate(root: Path) -> list[str]:
    path = root / MANIFEST
    if not path.is_file():
        return [f"missing {MANIFEST}"]
    entries = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(entries, list) or not entries:
        return ["MANIFEST.json must be a non-empty list"]
    problems: list[str] = []
    for entry in entries:
        relative = str(entry.get("path", ""))
        if not relative or Path(relative).is_absolute() or ".." in Path(relative).parts:
            problems.append(f"unsafe fixture path {relative!r}")
            continue
        fixture = root / ROOT / relative
        if not fixture.is_file():
            problems.append(f"{relative}: missing fixture")
            continue
        data = fixture.read_bytes()
        sha = hashlib.sha256(data).hexdigest()
        if sha != entry.get("sha256"):
            problems.append(f"{relative}: sha256 mismatch")
        if len(data) != entry.get("bytes"):
            problems.append(f"{relative}: byte size mismatch")
        count = entry.get("subtest_count_expected", 0)
        if not isinstance(count, int) or count < 0:
            problems.append(f"{relative}: invalid subtest_count_expected")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"WPT FIXTURE: {problem}", file=sys.stderr)
    if problems:
        return 1
    entries = json.loads((args.root.resolve() / MANIFEST).read_text(encoding="utf-8"))
    print(f"WPT fixtures OK ({len(entries)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
