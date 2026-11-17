#!/usr/bin/env python3
"""Turn an lcov tracefile into a coverage summary and a static badge (#962, #969).

`cargo llvm-cov --lcov` writes the standard lcov records. This script reads them,
computes line and function coverage over the workspace, prints a short report and
(carefully) regenerates `docs/status/badge-coverage.svg` next to the other generated
badges, so the README badge is a real number instead of a hand-edited string.

Badges are only rewritten when the rounded percentage changes, which keeps a
scheduled coverage run from touching the repository every week for nothing.

Usage:
    python3 tools/coverage_report.py target/lcov.info            # report only
    python3 tools/coverage_report.py target/lcov.info --write     # report + badge
    python3 tools/coverage_report.py target/lcov.info --min 70    # exit 1 below 70 %
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BADGE_PATH = REPO_ROOT / "docs" / "status" / "badge-coverage.svg"


@dataclass
class Totals:
    lines_found: int = 0
    lines_hit: int = 0
    functions_found: int = 0
    functions_hit: int = 0

    @property
    def line_pct(self) -> float:
        return (self.lines_hit / self.lines_found * 100.0) if self.lines_found else 0.0

    @property
    def function_pct(self) -> float:
        return (self.functions_hit / self.functions_found * 100.0) if self.functions_found else 0.0


def parse_lcov(path: Path) -> Totals:
    totals = Totals()
    for raw in path.read_text(encoding="utf-8").splitlines():
        if raw.startswith("LF:"):
            totals.lines_found += int(raw[3:])
        elif raw.startswith("LH:"):
            totals.lines_hit += int(raw[3:])
        elif raw.startswith("FNF:"):
            totals.functions_found += int(raw[4:])
        elif raw.startswith("FNH:"):
            totals.functions_hit += int(raw[4:])
    return totals


def badge_colour(pct: float) -> str:
    if pct >= 90.0:
        return "#1f6feb"
    if pct >= 75.0:
        return "#2ea043"
    if pct >= 60.0:
        return "#d29922"
    return "#cf222e"


def render_badge(pct: float) -> str:
    label = "coverage"
    value = f"{pct:.1f}%"
    # Widths mirror the generated badges in docs/status (DejaVu Sans 11 px).
    left = 22 + 6 * len(label)
    right = 20 + 7 * len(value)
    total = left + right
    colour = badge_colour(pct)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{total}" height="20" viewBox="0 0 {total} 20" role="img" aria-label="{label}: {value}">
  <title>{label}: {value}</title>
  <rect width="{left}" height="20" fill="#555555"/>
  <rect x="{left}" width="{right}" height="20" fill="{colour}"/>
  <g fill="#ffffff" text-anchor="middle" font-family="DejaVu Sans, Verdana, sans-serif" font-size="11">
    <text x="{left // 2}" y="14">{label}</text>
    <text x="{left + right // 2}" y="14">{value}</text>
  </g>
</svg>
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lcov", type=Path, help="lcov tracefile produced by cargo llvm-cov")
    parser.add_argument("--write", action="store_true", help="regenerate the coverage badge")
    parser.add_argument("--min", type=float, default=None, help="fail below this line percentage")
    args = parser.parse_args()

    try:
        totals = parse_lcov(args.lcov)
    except OSError as exc:
        print(f"error: cannot read {args.lcov}: {exc}", file=sys.stderr)
        return 2
    if not totals.lines_found:
        print(f"error: {args.lcov} has no LF/LH records", file=sys.stderr)
        return 2

    print(
        f"lines {totals.lines_hit}/{totals.lines_found} ({totals.line_pct:.2f} %), "
        f"functions {totals.functions_hit}/{totals.functions_found} ({totals.function_pct:.2f} %)"
    )

    if args.write:
        svg = render_badge(totals.line_pct)
        previous = BADGE_PATH.read_text(encoding="utf-8") if BADGE_PATH.exists() else ""
        if previous.strip() == svg.strip():
            print(f"badge unchanged ({BADGE_PATH.relative_to(REPO_ROOT)})")
        else:
            BADGE_PATH.write_text(svg, encoding="utf-8")
            print(f"wrote {BADGE_PATH.relative_to(REPO_ROOT)}")

    if args.min is not None and totals.line_pct < args.min:
        print(f"coverage {totals.line_pct:.2f} % is below the required {args.min:g} %")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
