#!/usr/bin/env python3
"""Validate docs/status/real_sites.toml without rendering or compiling.

Checks unique site ids, well-formed http(s) URLs, safe snapshot file names and
positive ``min_items``. Python only.

Usage:
    python3 tools/check_real_site_manifest.py [--manifest docs/status/real_sites.toml]
"""
from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path
from urllib.parse import urlparse

DEFAULT_MANIFEST = Path("docs/status/real_sites.toml")


def validate(manifest: dict) -> list[str]:
    """Return problems with the manifest; an empty list means it is sound."""
    sites = manifest.get("site", [])
    if not sites:
        return ["manifest has no [[site]] entries"]
    problems: list[str] = []
    seen: set[str] = set()
    for index, site in enumerate(sites):
        label = f"site[{index}]"
        site_id = str(site.get("id", "")).strip()
        if not site_id:
            problems.append(f"{label}: missing id")
        elif site_id in seen:
            problems.append(f"{site_id}: duplicate id")
        else:
            seen.add(site_id)
        label = site_id or label

        if not str(site.get("name", "")).strip():
            problems.append(f"{label}: missing name")

        url = str(site.get("url", ""))
        parsed = urlparse(url)
        if parsed.scheme not in ("http", "https") or not parsed.netloc:
            problems.append(f"{label}: url must be an absolute http(s) URL ({url!r})")

        snapshot = str(site.get("snapshot", f"{site_id}.html"))
        if not snapshot or Path(snapshot).name != snapshot or snapshot in (".", ".."):
            problems.append(f"{label}: unsafe snapshot path {snapshot!r}")
        elif not snapshot.lower().endswith(".html"):
            problems.append(f"{label}: snapshot must be an .html file ({snapshot!r})")

        minimum = site.get("min_items", 0)
        if not isinstance(minimum, int) or isinstance(minimum, bool) or minimum <= 0:
            problems.append(f"{label}: min_items must be a positive integer ({minimum!r})")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args(argv)
    try:
        with args.manifest.open("rb") as handle:
            manifest = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"check_real_site_manifest: {error}", file=sys.stderr)
        return 1
    problems = validate(manifest)
    for problem in problems:
        print(f"MANIFEST: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"real-site manifest OK ({len(manifest['site'])} sites)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
