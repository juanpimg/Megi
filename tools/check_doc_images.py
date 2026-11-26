#!/usr/bin/env python3
"""Check that images referenced from docs Markdown exist (Python only).

Handles both repository-root paths (``docs/images/x.png``) and paths relative
to the Markdown file (``images/x.png`` inside ``docs/``).

Usage:
    python3 tools/check_doc_images.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

IMAGE_PATH = re.compile(r"(?:docs/)?images/[A-Za-z0-9_./-]+\.(?:png|jpe?g|svg|webp|gif)")
DOCS = Path("docs")


def extract_refs(text: str) -> set[str]:
    return set(IMAGE_PATH.findall(text))


def validate(root: Path) -> list[str]:
    docs = root / DOCS
    if not docs.is_dir():
        return [f"missing {DOCS}"]
    problems: list[str] = []
    refs: set[str] = set()
    for doc in sorted(docs.rglob("*.md")):
        for raw in extract_refs(doc.read_text(encoding="utf-8")):
            relative = raw if raw.startswith("docs/") else (doc.parent / raw).relative_to(root).as_posix()
            refs.add(relative)
    if not refs:
        return ["no documentation images referenced"]
    for relative in sorted(refs):
        if not (root / relative).is_file():
            problems.append(f"{relative}: referenced image is missing")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"DOC IMAGE: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("documentation images OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
