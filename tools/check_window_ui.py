#!/usr/bin/env python3
"""Compile-free visual parity gate for the Megi window shell (Pillow only).

Two modes:

* ``--compare A.png B.png --max-mae N`` prints ``MAE=...`` and exits 0 when the
  mean absolute channel difference is at most N, 1 otherwise.
* ``--check [--captures DIR]`` compares the three Chromium references in
  ``tests/fixtures/chrome/ui/`` against Megi captures (default:
  ``target/ui-captures/``) with the per-site thresholds below. Missing or
  invalid inputs are errors: this tool never synthesizes a reference, a
  capture or a measurement.

MAE = mean(|dR| + |dG| + |dB|) / 3 over equally sized images.

Usage:
    python3 tools/check_window_ui.py --compare A.png B.png --max-mae 18
    python3 tools/check_window_ui.py --check [--captures DIR] [--references DIR]
"""
from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageStat, UnidentifiedImageError

ROOT = Path(__file__).resolve().parent.parent
REFERENCES_DIR = ROOT / "tests" / "fixtures" / "chrome" / "ui"
CAPTURES_DIR = ROOT / "target" / "ui-captures"
# Thresholds from the approved plan (2026-11-26 UI quality unblock plan, Task 5).
THRESHOLDS = {"hn": 18, "rust": 14, "wikipedia": 22}


class UiGateError(Exception):
    """An invalid or missing input; reported as a failed gate, never guessed."""


def load_png(path: Path) -> Image.Image:
    """Read a real PNG as RGB; raise rather than fabricate pixels."""
    if not path.is_file():
        raise UiGateError(f"missing file: {path}")
    if path.suffix.lower() != ".png":
        raise UiGateError(f"invalid format: {path} is not a .png file")
    try:
        with Image.open(path) as image:
            if image.format != "PNG":
                raise UiGateError(f"invalid PNG: {path} has format {image.format!r}")
            image.load()
            return image.convert("RGB")
    except (UnidentifiedImageError, OSError, ValueError) as error:
        raise UiGateError(f"invalid PNG: {path} ({error})") from error


def mae(left: Image.Image, right: Image.Image) -> float:
    """Mean of the three per-channel mean absolute differences."""
    if left.size != right.size:
        raise UiGateError(
            f"size mismatch: {left.size[0]}x{left.size[1]} vs "
            f"{right.size[0]}x{right.size[1]}"
        )
    diff = ImageChops.difference(left, right)
    return sum(ImageStat.Stat(diff).mean) / 3.0


def compare_files(left: Path, right: Path) -> float:
    """MAE between two PNG files, failing closed on bad inputs."""
    return mae(load_png(left), load_png(right))


def check_all(references_dir: Path, captures_dir: Path) -> list[str]:
    """Compare every site; return the list of human-readable failures."""
    errors: list[str] = []
    for site, threshold in THRESHOLDS.items():
        reference = references_dir / f"{site}.png"
        capture = captures_dir / f"{site}.png"
        if not reference.is_file():
            errors.append(
                f"{site}: missing reference: {reference} "
                "(real Chromium captures are not versioned; see docs/UI-TARGET.md)"
            )
            continue
        if not capture.is_file():
            errors.append(
                f"{site}: missing capture: {capture} "
                "(run tools/capture_window_ui.sh; no capture is fabricated)"
            )
            continue
        try:
            value = compare_files(reference, capture)
        except UiGateError as error:
            errors.append(f"{site}: {error}")
            continue
        print(f"{site} MAE={value:.3f} limit={threshold}")
        if value > threshold:
            errors.append(f"{site}: MAE {value:.3f} above threshold {threshold}")
    return errors


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--compare", nargs=2, metavar=("A.png", "B.png"))
    mode.add_argument("--check", action="store_true")
    parser.add_argument("--max-mae", type=float, metavar="N")
    parser.add_argument("--captures", type=Path, default=CAPTURES_DIR, metavar="DIR")
    parser.add_argument("--references", type=Path, default=REFERENCES_DIR, metavar="DIR")
    args = parser.parse_args(argv)

    if args.compare:
        if args.max_mae is None or not math.isfinite(args.max_mae) or args.max_mae < 0:
            parser.error("--compare requires a finite --max-mae >= 0")
        try:
            value = compare_files(Path(args.compare[0]), Path(args.compare[1]))
        except UiGateError as error:
            print(f"check_window_ui: {error}", file=sys.stderr)
            return 1
        print(f"MAE={value:.3f}")
        if value > args.max_mae:
            print(
                f"check_window_ui: MAE {value:.3f} above threshold {args.max_mae}",
                file=sys.stderr,
            )
            return 1
        return 0

    if args.max_mae is not None:
        parser.error("--max-mae only applies to --compare")
    errors = check_all(args.references, args.captures)
    for error in errors:
        print(f"check_window_ui: {error}", file=sys.stderr)
    if errors:
        print("check_window_ui: visual parity NOT verified", file=sys.stderr)
        return 1
    print("window UI parity OK (3/3 sites within thresholds)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
