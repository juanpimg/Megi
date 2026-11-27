#!/usr/bin/env python3
"""Compare two rendered PNGs and report visual parity (no Rust compilation needed).

Usage:
    python3 tools/visual_parity.py a.png b.png [--threshold 16] [--side out.png] [--json]

A pixel counts as changed when its largest per-channel absolute difference is
greater than ``--threshold``. ``--side`` writes a side-by-side image. Requires
Pillow; the engine and reference capture are produced separately (see
``docs/VISUAL-PARITY.md``).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    from PIL import Image, ImageChops, ImageStat
except ModuleNotFoundError:  # pragma: no cover - depends on host packages
    print("visual_parity: Pillow is required (python3-pil)", file=sys.stderr)
    sys.exit(2)


def compare_images(first: Path, second: Path, threshold: int = 16) -> dict:
    """Return pixel-difference metrics for two images of the same size."""
    if threshold < 0:
        raise ValueError("threshold must be >= 0")
    with Image.open(first) as left, Image.open(second) as right:
        a = left.convert("RGB")
        b = right.convert("RGB")
    if a.size != b.size:
        raise ValueError(f"image sizes differ: {a.size} vs {b.size}")

    diff = ImageChops.difference(a, b)
    # #2447: docs promise max(|dR|,|dG|,|dB|), but convert("L") measures a
    # luminance-weighted mean and hides single-channel differences. Combine
    # channels with lighter() so the mask keeps the maximum absolute delta.
    channels = diff.split()
    maximum = ImageChops.lighter(ImageChops.lighter(channels[0], channels[1]), channels[2])
    mask = maximum.point(lambda value: 255 if value > threshold else 0)
    changed = sum(1 for pixel in mask.getdata() if pixel)
    stat = ImageStat.Stat(diff)
    total = a.width * a.height
    return {
        "size": [a.width, a.height],
        "threshold": threshold,
        "changed_pixels": changed,
        "changed_pct": round(100.0 * changed / total, 2) if total else 0.0,
        "mean_abs_diff": round(sum(stat.mean) / 3, 3),
        "diff_bbox": mask.getbbox(),
    }


def side_by_side(first: Path, second: Path, destination: Path) -> None:
    """Write both images next to each other on a white canvas."""
    with Image.open(first) as left, Image.open(second) as right:
        a = left.convert("RGB")
        b = right.convert("RGB")
    if a.size != b.size:
        raise ValueError(f"image sizes differ: {a.size} vs {b.size}")
    canvas = Image.new("RGB", (a.width * 2, a.height), "white")
    canvas.paste(a, (0, 0))
    canvas.paste(b, (a.width, 0))
    canvas.save(destination)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("first", type=Path, help="first PNG (e.g. megi render)")
    parser.add_argument("second", type=Path, help="second PNG (e.g. Chrome capture)")
    parser.add_argument("--threshold", type=int, default=16)
    parser.add_argument("--side", type=Path, help="write a side-by-side PNG")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    try:
        report = compare_images(args.first, args.second, args.threshold)
        if args.side:
            side_by_side(args.first, args.second, args.side)
    except (OSError, ValueError) as error:
        print(f"visual_parity: {error}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(report))
    else:
        print(
            f"{report['changed_pct']}% changed "
            f"({report['changed_pixels']} px, mean {report['mean_abs_diff']}, "
            f"bbox {report['diff_bbox']})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
