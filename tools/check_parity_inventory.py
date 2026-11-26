#!/usr/bin/env python3
"""Check that the parity artifact inventory matches the filesystem (Python only).

``check_parity_ledger.py`` validates the declared paths against the audit
ledger; this tool validates the declaration itself: every parity document,
fixture and tool must be in ``DEFAULT_PATHS`` and every declared path must exist.

Usage:
    python3 tools/check_parity_inventory.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    from check_parity_ledger import DEFAULT_PATHS
except ModuleNotFoundError:  # imported as tools.check_parity_inventory from the root
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from check_parity_ledger import DEFAULT_PATHS

DOCS = ("docs/VISUAL-PARITY.md", "docs/DESLOP-AUDIT.md")
FIXTURE_DIR = "tests/fixtures/visual-parity"
TOOL_DIR = "tools"


def discover(root: Path) -> set[str]:
    """Return the relative paths of every visual-parity artifact in ``root``."""
    found: set[str] = set()
    for relative in DOCS:
        if (root / relative).is_file():
            found.add(relative)
    fixtures = root / FIXTURE_DIR
    if fixtures.is_dir():
        for path in fixtures.rglob("*"):
            if path.is_file():
                found.add(path.relative_to(root).as_posix())
    tools = root / TOOL_DIR
    if tools.is_dir():
        for path in tools.glob("*.py"):
            if "parity" in path.name:
                found.add(f"{TOOL_DIR}/{path.name}")
    return found


def validate(root: Path, declared: tuple[str, ...]) -> list[str]:
    """Return paths missing from the declaration or declared but absent."""
    actual = discover(root)
    problems = [
        f"{path}: parity artifact is not declared in DEFAULT_PATHS"
        for path in sorted(actual - set(declared))
    ]
    problems.extend(
        f"{path}: declared in DEFAULT_PATHS but the file does not exist"
        for path in sorted(set(declared) - actual)
    )
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    problems = validate(root, DEFAULT_PATHS)
    for problem in problems:
        print(f"INVENTORY: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"parity artifact inventory OK ({len(DEFAULT_PATHS)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
