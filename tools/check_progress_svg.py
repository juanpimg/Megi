#!/usr/bin/env python3
"""Check docs/status/progress.svg against PROGRESS.md (Python only).

Usage:
    python3 tools/check_progress_svg.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SNAPSHOT = re.compile(
    r"commit `?([0-9a-f]+)`?.*?\*\*([\d,]+)\*\* tests · conformance \*\*(\w+)\*\* · "
    r"\*\*(\d+)\*\* crates · \*\*([\d,]+)\*\* lines of Rust"
)
FOOTER = re.compile(
    r"commit ([0-9a-f]+) \([^)]*\) · (\d+) crates · ([\d,]+) tests · "
    r"conformance (\w+) · ([\d,]+) lines of Rust"
)


def validate(root: Path) -> list[str]:
    md_path, svg_path = root / "PROGRESS.md", root / "docs/status/progress.svg"
    if not md_path.is_file() or not svg_path.is_file():
        return ["missing PROGRESS.md or docs/status/progress.svg"]
    md = SNAPSHOT.search(md_path.read_text(encoding="utf-8"))
    svg = FOOTER.search(svg_path.read_text(encoding="utf-8"))
    if not md:
        return ["PROGRESS.md: snapshot line is missing"]
    if not svg:
        return ["progress.svg: footer snapshot is missing"]
    problems: list[str] = []
    pairs = (
        ("commit", md.group(1), svg.group(1)),
        ("tests", md.group(2), svg.group(3)),
        ("crates", md.group(4), svg.group(2)),
        ("conformance", md.group(3), svg.group(4)),
        ("rust_lines", md.group(5), svg.group(5)),
    )
    for name, expected, actual in pairs:
        if expected != actual:
            problems.append(f"progress.svg: {name}={actual} but PROGRESS.md says {expected}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"PROGRESS SVG: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("progress SVG consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
