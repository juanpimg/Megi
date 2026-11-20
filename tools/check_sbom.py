#!/usr/bin/env python3
"""Validate that a release SBOM directory holds at least one valid JSON BOM (#1837).

The release workflow used to generate SBOM files with ``|| true`` chains and an
``if-no-files-found: warn`` upload, so an empty or malformed ``sbom-dist`` could be
published silently. This tool turns that into a hard failure: it exits 1 unless
every ``*.json`` in the directory parses and at least one is a CycloneDX document.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print("usage: check_sbom.py <sbom-dir>", file=sys.stderr)
        return 2
    directory = Path(argv[0])
    if not directory.is_dir():
        print(f"error: SBOM directory does not exist: {directory}", file=sys.stderr)
        return 1

    files = sorted(directory.glob("*.json"))
    if not files:
        print(f"error: no SBOM JSON files found in {directory}", file=sys.stderr)
        return 1

    valid = 0
    for path in files:
        try:
            doc = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            print(f"error: invalid SBOM JSON {path}: {exc}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict) or doc.get("bomFormat") != "CycloneDX":
            print(
                f"error: {path} is not a CycloneDX BOM (missing bomFormat)",
                file=sys.stderr,
            )
            return 1
        valid += 1

    print(f"validated {valid} CycloneDX SBOM JSON file(s) in {directory}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
