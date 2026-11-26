#!/usr/bin/env python3
"""Check Cargo.toml version against CHANGELOG.md headings (Python only).

Usage:
    python3 tools/check_changelog_version.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

CARGO = Path("Cargo.toml")
CHANGELOG = Path("CHANGELOG.md")
RELEASE = re.compile(r"^## \[(\d+\.\d+\.\d+)\](?: - (\d{4}-\d{2}-\d{2}))?", re.M)


def validate(root: Path) -> list[str]:
    cargo_path, changelog_path = root / CARGO, root / CHANGELOG
    if not cargo_path.is_file() or not changelog_path.is_file():
        return [f"missing {CARGO} or {CHANGELOG}"]
    with cargo_path.open("rb") as handle:
        version = str(tomllib.load(handle).get("workspace", {}).get("package", {}).get("version", ""))
    text = changelog_path.read_text(encoding="utf-8")
    releases = RELEASE.findall(text)
    problems: list[str] = []
    if not version:
        problems.append("Cargo.toml: missing workspace.package.version")
    if not releases:
        return problems + ["CHANGELOG.md: no version headings found"]
    versions = [item[0] for item in releases]
    if len(versions) != len(set(versions)):
        problems.append("CHANGELOG.md: duplicate version headings")
    if version not in versions:
        problems.append(f"CHANGELOG.md: version {version} from Cargo.toml is not documented")
    if "## [Unreleased]" not in text:
        problems.append("CHANGELOG.md: missing [Unreleased] section")
    for value, date in releases:
        if value != versions[0] and not date:
            problems.append(f"CHANGELOG.md: release {value} has no ISO date")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"CHANGELOG: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("changelog version consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
