#!/usr/bin/env python3
"""Validate the visual-parity baseline against the real-site manifest (Python only).

Checks that every site in ``docs/status/real_sites.toml`` has a metrics entry
and that each entry follows the schema produced by ``tools/visual_parity.py``.

Usage:
    python3 tools/check_parity_schema.py
        [--baseline tests/fixtures/visual-parity/baseline.json]
        [--manifest docs/status/real_sites.toml]
"""
from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path

DEFAULT_BASELINE = Path("tests/fixtures/visual-parity/baseline.json")
DEFAULT_MANIFEST = Path("docs/status/real_sites.toml")
REQUIRED = {
    "size": list,
    "threshold": int,
    "changed_pixels": int,
    "changed_pct": (int, float),
    "mean_abs_diff": (int, float),
    "diff_bbox": (list, type(None)),
}


def validate(baseline: dict, manifest: dict) -> list[str]:
    """Return validation problems; an empty list means the baseline is sound."""
    problems: list[str] = []
    if not isinstance(baseline, dict):
        return ["baseline must be a JSON object"]

    expected_ids = {str(site.get("id")) for site in manifest.get("site", [])}
    if not expected_ids:
        problems.append("manifest has no [[site]] entries")
    for site_id in sorted(expected_ids - baseline.keys()):
        problems.append(f"{site_id}: missing from baseline")

    for site, metrics in baseline.items():
        if site not in expected_ids:
            problems.append(f"{site}: not present in the real-site manifest")
        if not isinstance(metrics, dict):
            problems.append(f"{site}: metrics must be an object")
            continue
        for key, expected_type in REQUIRED.items():
            if key not in metrics:
                problems.append(f"{site}: missing key {key}")
            elif not isinstance(metrics[key], expected_type):
                problems.append(f"{site}: {key} has wrong type {type(metrics[key]).__name__}")
        size = metrics.get("size")
        if isinstance(size, list) and (
            len(size) != 2 or not all(isinstance(v, int) and v > 0 for v in size)
        ):
            problems.append(f"{site}: size must be [width, height] with positive ints")
        bbox = metrics.get("diff_bbox")
        if isinstance(bbox, list) and (
            len(bbox) != 4 or not all(isinstance(v, int) and v >= 0 for v in bbox)
        ):
            problems.append(f"{site}: diff_bbox must be [x0, y0, x1, y1]")
        pct = metrics.get("changed_pct")
        if isinstance(pct, (int, float)) and not 0 <= pct <= 100:
            problems.append(f"{site}: changed_pct out of range ({pct})")
        pixels = metrics.get("changed_pixels")
        if isinstance(pixels, int) and isinstance(size, list) and len(size) == 2:
            if not 0 <= pixels <= size[0] * size[1]:
                problems.append(f"{site}: changed_pixels exceeds the viewport")
        diff = metrics.get("mean_abs_diff")
        if isinstance(diff, (int, float)) and diff < 0:
            problems.append(f"{site}: mean_abs_diff must be >= 0")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    args = parser.parse_args(argv)
    try:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        with args.manifest.open("rb") as handle:
            manifest = tomllib.load(handle)
    except (OSError, json.JSONDecodeError, tomllib.TOMLDecodeError, ValueError) as error:
        print(f"check_parity_schema: {error}", file=sys.stderr)
        return 1
    problems = validate(baseline, manifest)
    for problem in problems:
        print(f"INVALID: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"parity baseline schema OK ({len(baseline)} sites)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
