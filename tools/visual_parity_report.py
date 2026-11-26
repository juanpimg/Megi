#!/usr/bin/env python3
"""Build a Markdown/JSON parity report from *-megi.png / *-chrome.png pairs.

No Rust compilation and no browser automation: it consumes captures produced
separately (see docs/VISUAL-PARITY.md) and reuses tools/visual_parity.py.

Usage:
    python3 tools/visual_parity_report.py DIRECTORY [--site example --site rust]
                                     [--json OUTPUT.json] [--markdown OUTPUT.md]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    from visual_parity import compare_images
except ModuleNotFoundError:  # imported as tools.visual_parity_report from the repo root
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from visual_parity import compare_images

SUFFIXES = ("megi", "chrome")


def discover(directory: Path, sites: list[str] | None = None) -> list[tuple[str, Path, Path]]:
    """Return (site, megi_png, chrome_png) pairs found in ``directory``."""
    if not directory.is_dir():
        raise ValueError(f"not a directory: {directory}")
    pairs: list[tuple[str, Path, Path]] = []
    names = sites or sorted(
        path.name[: -len("-megi.png")]
        for path in directory.glob("*-megi.png")
    )
    for site in names:
        megi = directory / f"{site}-megi.png"
        chrome = directory / f"{site}-chrome.png"
        if not megi.is_file() or not chrome.is_file():
            raise ValueError(f"missing capture pair for {site}: {megi.name} / {chrome.name}")
        pairs.append((site, megi, chrome))
    if not pairs:
        raise ValueError(f"no *-megi.png captures in {directory}")
    return pairs


def build_report(directory: Path, sites: list[str] | None = None, threshold: int = 16) -> dict:
    """Compute the metrics for every discovered capture pair."""
    report: dict[str, dict] = {}
    for site, megi, chrome in discover(directory, sites):
        report[site] = compare_images(megi, chrome, threshold)
    return report


def markdown(report: dict) -> str:
    """Render the report as a Markdown table."""
    lines = [
        "| Sitio | Píxeles cambiados | % | Diff media | Bbox |",
        "|---|---:|---:|---:|---|",
    ]
    for site, metrics in report.items():
        bbox = metrics["diff_bbox"]
        bbox_text = ", ".join(str(value) for value in bbox) if bbox else "—"
        lines.append(
            f"| {site} | {metrics['changed_pixels']} | {metrics['changed_pct']} "
            f"| {metrics['mean_abs_diff']} | {bbox_text} |"
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="directory with *-megi.png / *-chrome.png")
    parser.add_argument("--site", action="append", help="site name (may be repeated)")
    parser.add_argument("--threshold", type=int, default=16)
    parser.add_argument("--json", type=Path, help="write JSON report")
    parser.add_argument("--markdown", type=Path, help="write Markdown report")
    args = parser.parse_args(argv)

    try:
        report = build_report(args.directory, args.site, args.threshold)
        if args.json:
            args.json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        if args.markdown:
            args.markdown.write_text(markdown(report), encoding="utf-8")
    except (OSError, ValueError) as error:
        print(f"visual_parity_report: {error}", file=sys.stderr)
        return 1
    print(markdown(report), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
