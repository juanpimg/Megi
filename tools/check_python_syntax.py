#!/usr/bin/env python3
"""Parse every tools/*.py with ast (no bytecode, no cargo, no Rust build).

Usage:
    python3 tools/check_python_syntax.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import ast
import sys
from pathlib import Path


def validate(root: Path) -> list[str]:
    """Return syntax problems found in the Python tools."""
    tools = root / "tools"
    if not tools.is_dir():
        return [f"missing directory {tools}"]
    files = sorted(tools.glob("*.py"))
    if not files:
        return ["no tools/*.py files found"]
    problems: list[str] = []
    for path in files:
        try:
            ast.parse(path.read_text(encoding="utf-8"), filename=path.name)
        except (OSError, SyntaxError, ValueError) as error:
            problems.append(f"{path.name}: {error}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    problems = validate(root)
    for problem in problems:
        print(f"SYNTAX: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"python syntax OK ({len(list((root / 'tools').glob('*.py')))} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
