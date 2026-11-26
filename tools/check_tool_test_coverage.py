#!/usr/bin/env python3
"""Check that every tools/*.py has a test module or a documented exemption.

Python only. Modules named ``test_*.py`` are their own tests. Everything else
needs ``tools/test_<name>.py`` or an entry with a reason in
``tests/fixtures/tool-test-exemptions.toml``.

Usage:
    python3 tools/check_tool_test_coverage.py [--root /path/to/repo]
        [--manifest tests/fixtures/tool-test-exemptions.toml]
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

DEFAULT_MANIFEST = Path("tests/fixtures/tool-test-exemptions.toml")


def discover_tools(root: Path) -> set[str]:
    tools = root / "tools"
    return {path.name for path in tools.glob("*.py")} if tools.is_dir() else set()


def read_exemptions(path: Path) -> dict[str, str]:
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    return {
        entry["name"]: entry["reason"]
        for entry in data.get("exemption", [])
        if entry.get("name") and entry.get("reason")
    }


def validate(tools: set[str], exemptions: dict[str, str]) -> list[str]:
    """Return uncovered tools and stale/empty exemption entries."""
    if not tools:
        return ["no tools/*.py files found"]
    problems: list[str] = []
    for name in sorted(tools):
        if name.startswith("test_"):
            continue
        test = f"test_{Path(name).stem}.py"
        if test in tools:
            continue
        reason = exemptions.get(name)
        if reason:
            if not reason.strip():
                problems.append(f"{name}: exemption has an empty reason")
        else:
            problems.append(f"{name}: no {test} and no documented exemption")
    for name in sorted(set(exemptions) - tools):
        problems.append(f"{name}: exemption is stale (tool does not exist)")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    manifest = args.manifest if args.manifest.is_absolute() else root / args.manifest
    try:
        exemptions = read_exemptions(manifest)
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"check_tool_test_coverage: {error}", file=sys.stderr)
        return 1
    problems = validate(discover_tools(root), exemptions)
    for problem in problems:
        print(f"TEST COVERAGE: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"tool test coverage OK ({len(discover_tools(root))} tools)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
