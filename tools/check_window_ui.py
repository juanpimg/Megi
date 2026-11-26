#!/usr/bin/env python3
"""Compile-free visual parity gate for the window shell (WP-USER-ui-gate).

Compares 1000x700 Megi screenshots against Chromium references using the mean
absolute error over the three RGB channels. Thresholds come from Task 5 of
docs/superpowers/plans/2026-11-26-ui-quality-unblock-plan.md: HN 18, rust 14,
wikipedia 22. No number is invented here.

Usage:
    python3 tools/check_window_ui.py --compare MEGI.png CHROME.png --max-mae 18
    python3 tools/check_window_ui.py --check [--captures DIR]
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat

ROOT = Path(__file__).resolve().parent.parent
REFERENCES = ROOT / "tests" / "fixtures" / "chrome" / "ui"
CAPTURES = REFERENCES / "captures"
# Task 5 thresholds; baseline MAE 15.48 / 10.62 / 19.22 plus margin.
THRESHOLDS: dict[str, float] = {"hn": 18.0, "rust": 14.0, "wikipedia": 22.0}


def mae(a: Image.Image, b: Image.Image) -> float:
    """mean(|dR| + |dG| + |dB|) / 3 over equally sized RGB images."""
    if a.size != b.size:
        raise ValueError(f"size mismatch: {a.size} != {b.size}")
    a_rgb, b_rgb = a.convert("RGB"), b.convert("RGB")
    per_channel = (
        ImageStat.Stat(ImageChops.difference(left, right)).mean[0]
        for left, right in zip(a_rgb.split(), b_rgb.split())
    )
    return sum(per_channel) / 3.0


def _load(path: Path, label: str) -> Image.Image | None:
    if not path.is_file():
        print(f"{label}: {path} (generate it with tools/capture_window_ui.sh)", file=sys.stderr)
        return None
    try:
        with Image.open(path) as image:
            return image.convert("RGB")
    except (OSError, ValueError) as exc:
        print(f"{label}: {path}: {exc}", file=sys.stderr)
        return None


def compare(a: Path, b: Path, max_mae: float) -> int:
    left, right = _load(a, "missing capture"), _load(b, "missing reference")
    if left is None or right is None:
        return 1
    try:
        value = mae(left, right)
    except ValueError as exc:
        print(f"size mismatch: {a} vs {b}: {exc}", file=sys.stderr)
        return 1
    print(f"MAE={value:.3f}")
    return 0 if value <= max_mae else 1


def check(captures_dir: Path) -> int:
    failures = 0
    for site, threshold in THRESHOLDS.items():
        reference = _load(REFERENCES / f"{site}.png", "missing reference")
        capture = _load(captures_dir / f"{site}.png", "missing capture")
        if reference is None or capture is None:
            failures += 1
            continue
        try:
            value = mae(capture, reference)
        except ValueError as exc:
            print(f"{site}: size mismatch: {exc}", file=sys.stderr)
            failures += 1
            continue
        ok = value <= threshold
        print(f"{site}: MAE={value:.3f} max={threshold:.0f} {'PASS' if ok else 'FAIL'}")
        failures += 0 if ok else 1
    total = len(THRESHOLDS)
    print(f"window_ui: {'PASS' if not failures else 'FAIL'} {total - failures}/{total}")
    return 1 if failures else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compare", nargs=2, metavar=("MEGI", "CHROME"))
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--captures", type=Path, default=CAPTURES, metavar="DIR")
    parser.add_argument("--max-mae", type=float, default=18.0, metavar="N")
    args = parser.parse_args(argv)
    if bool(args.compare) == args.check:
        parser.error("use exactly one of --compare A B or --check")
    if args.compare:
        return compare(Path(args.compare[0]), Path(args.compare[1]), args.max_mae)
    return check(args.captures)


if __name__ == "__main__":
    sys.exit(main())
