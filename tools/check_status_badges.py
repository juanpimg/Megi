#!/usr/bin/env python3
"""Check status badges referenced by README/PROGRESS exist and agree with the gate.

Python only; no cargo. The conformance badge must agree with [gate].status in
docs/status/progress.toml.

Usage:
    python3 tools/check_status_badges.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

DOCS = (Path("README.md"), Path("PROGRESS.md"))
BADGE = re.compile(r"docs/status/badge-[A-Za-z0-9-]+\.svg")
LABEL = re.compile(r'aria-label="([^"]+)"')


def validate(root: Path) -> list[str]:
    problems: list[str] = []
    refs: set[str] = set()
    for doc in DOCS:
        path = root / doc
        if not path.is_file():
            problems.append(f"missing {doc}")
            continue
        refs.update(BADGE.findall(path.read_text(encoding="utf-8")))
    if not refs:
        problems.append("no status badge references found")
    for relative in sorted(refs):
        if not (root / relative).is_file():
            problems.append(f"{relative}: referenced badge is missing")
    conformance = root / "docs/status/badge-conformance.svg"
    manifest = root / "docs/status/progress.toml"
    if conformance.is_file() and manifest.is_file():
        with manifest.open("rb") as handle:
            expected = str(tomllib.load(handle).get("gate", {}).get("status", "")).upper()
        label = LABEL.search(conformance.read_text(encoding="utf-8"))
        if not label:
            problems.append("badge-conformance.svg: missing aria-label")
        elif expected and expected not in label.group(1).upper():
            problems.append(
                f"badge-conformance.svg: label {label.group(1)!r} disagrees with gate {expected!r}"
            )
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"STATUS BADGE: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("status badges consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
