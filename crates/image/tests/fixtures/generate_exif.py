#!/usr/bin/env python3
"""Legacy entry point for the EXIF orientation fixtures (#1365, #1652).

Fixture generation used to live here at 12x8, duplicating
``generate_exif_orientations.py`` (16x16) under the same ``exif_o1..o8.jpg``
names. Whichever script ran last silently replaced the fixtures, and the Rust
tests pin the canonical 16x16 geometry, so running this one broke them. There is
now a single canonical generator; this script only delegates to it and then
syncs the derived copies (render asset, fuzz corpus).

Run from the repository root:
    python3 crates/image/tests/fixtures/generate_exif.py

Requires: Pillow (PIL).
"""

from __future__ import annotations

import os
from pathlib import Path

# #1652: import the single canonical geometry source rather than redefining it.
from generate_exif_orientations import main as generate_canonical_fixtures

ROOT = Path(__file__).resolve().parents[4]
OUT_DIR = ROOT / "crates/image/tests/fixtures/jpeg"


def sync_copies(out_dir: Path, root: Path) -> None:
    render_exif = root / "tests/fixtures/render/assets/exif.jpg"
    if render_exif.exists():
        render_exif.write_bytes((out_dir / "exif_o6.jpg").read_bytes())
        print(f"synced {render_exif}")
    fuzz_dir = root / "fuzz/corpus/exif_parse"
    if fuzz_dir.exists():
        for orientation in range(1, 9):
            src = out_dir / f"exif_o{orientation}.jpg"
            dst = fuzz_dir / f"exif_o{orientation}.jpg"
            if dst.exists() or src.exists():
                dst.write_bytes(src.read_bytes())
        print(f"synced {fuzz_dir}/exif_o*.jpg")


def main() -> None:
    # The canonical generator resolves FIXTURES relative to the repository root.
    previous = Path.cwd()
    os.chdir(ROOT)
    try:
        generate_canonical_fixtures()
    finally:
        os.chdir(previous)
    sync_copies(OUT_DIR, ROOT)


if __name__ == "__main__":
    main()
