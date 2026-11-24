"""Seven WP generators fed by real artifacts (spec §4.3).

Conservative scaffold: no artifact parsers yet, so each generator returns an
empty list offline. The work-package-engine plan (Task 2) adds the parsers.
"""

from __future__ import annotations
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools.wp_schema import WorkPackage  # noqa: E402

SOURCES = ("wpt", "js262", "site", "fuzz", "audit", "depth", "user")

def generate(source: str, dry_run: bool = True) -> list[WorkPackage]:
    if source not in SOURCES:
        raise ValueError(f"unknown WP source: {source}")
    return []

def main(argv: list[str] | None = None) -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Generate work packages")
    parser.add_argument("source", choices=SOURCES + ("all",), nargs="?", default="all")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    selected = "all" if args.all else args.source
    sources = SOURCES if selected == "all" else (selected,)
    total = 0
    for source in sources:
        wps = generate(source, dry_run=args.dry_run)
        total += len(wps)
        print(f"{source}: {len(wps)} work packages")
    print(f"total: {total}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
