#!/usr/bin/env python3
"""Check that every snapshot referenced by the real-site manifest exists.

Python only: reads TOML with ``tomllib`` and checks the versioned HTML files
under ``tests/fixtures/real-sites/``. No renderer or cargo invocation.

Usage:
    python3 tools/check_real_site_snapshots.py
        [--manifest docs/status/real_sites.toml]
        [--fixtures tests/fixtures/real-sites]
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

DEFAULT_MANIFEST = Path("docs/status/real_sites.toml")
DEFAULT_FIXTURES = Path("tests/fixtures/real-sites")


def validate(manifest: dict, fixtures: Path) -> list[str]:
    """Return problems for missing or empty snapshot files."""
    problems: list[str] = []
    sites = manifest.get("site", [])
    if not sites:
        return ["manifest has no [[site]] entries"]
    for site in sites:
        site_id = str(site.get("id", "?"))
        snapshot = str(site.get("snapshot", f"{site_id}.html"))
        if not snapshot:
            problems.append(f"{site_id}: empty snapshot name")
            continue
        path = fixtures / snapshot
        if not path.is_file():
            problems.append(f"{site_id}: missing snapshot {path.as_posix()}")
        elif path.stat().st_size == 0:
            problems.append(f"{site_id}: empty snapshot {path.as_posix()}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--fixtures", type=Path, default=DEFAULT_FIXTURES)
    args = parser.parse_args(argv)
    try:
        with args.manifest.open("rb") as handle:
            manifest = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"check_real_site_snapshots: {error}", file=sys.stderr)
        return 1
    problems = validate(manifest, args.fixtures)
    for problem in problems:
        print(f"SNAPSHOT: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"real-site snapshots OK ({len(manifest['site'])} sites)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
