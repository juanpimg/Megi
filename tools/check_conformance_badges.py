#!/usr/bin/env python3
"""Check WPT/Test262 badges against docs/status/progress.toml (Python only).

Usage:
    python3 tools/check_conformance_badges.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

BADGES = {
    "wpt": (Path("docs/status/badge-wpt.svg"), re.compile(r"wpt:\s*(\d+)/(\d+)")),
    "test262": (Path("docs/status/badge-test262.svg"), re.compile(r"test262:\s*(\d+)/(\d+)")),
}


def validate(root: Path) -> list[str]:
    manifest = root / "docs/status/progress.toml"
    if not manifest.is_file():
        return ["missing docs/status/progress.toml"]
    with manifest.open("rb") as handle:
        data = tomllib.load(handle)
    problems: list[str] = []
    for name, (path, pattern) in BADGES.items():
        badge = root / path
        if not badge.is_file():
            problems.append(f"missing {path}")
            continue
        match = pattern.search(badge.read_text(encoding="utf-8"))
        if not match:
            problems.append(f"{path.name}: missing or malformed aria-label")
            continue
        block = data.get(name, {})
        shown = (int(match.group(1)), int(match.group(2)))
        expected = (block.get("passed"), block.get("total"))
        if shown != expected:
            problems.append(
                f"{path.name}: shows {shown[0]}/{shown[1]}, progress.toml says {expected[0]}/{expected[1]}"
            )
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"CONFORMANCE BADGE: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("conformance badges consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
