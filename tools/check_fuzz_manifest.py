#!/usr/bin/env python3
"""Check fuzz/Cargo.toml targets and their corpus directories (Python only).

Usage:
    python3 tools/check_fuzz_manifest.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

FUZZ = Path("fuzz")
MANIFEST = FUZZ / "Cargo.toml"


def validate(root: Path) -> list[str]:
    path = root / MANIFEST
    if not path.is_file():
        return [f"missing {MANIFEST}"]
    with path.open("rb") as handle:
        bins = tomllib.load(handle).get("bin", [])
    if not bins:
        return ["fuzz/Cargo.toml has no [[bin]] targets"]
    problems: list[str] = []
    for entry in bins:
        name = str(entry.get("name", ""))
        target = str(entry.get("path", ""))
        if not name or not target:
            problems.append(f"fuzz target has no name/path: {entry}")
            continue
        if not (root / FUZZ / target).is_file():
            problems.append(f"{name}: missing target source {target}")
        if Path(target).stem != name:
            problems.append(f"{name}: path {target} does not match target name")
        corpus = root / FUZZ / "corpus" / name
        if not corpus.is_dir():
            problems.append(f"{name}: missing corpus directory fuzz/corpus/{name}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"FUZZ MANIFEST: {problem}", file=sys.stderr)
    if problems:
        return 1
    count = len(tomllib.load((args.root.resolve() / MANIFEST).open("rb")).get("bin", []))
    print(f"fuzz manifest OK ({count} targets)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
