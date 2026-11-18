#!/usr/bin/env python3
"""Generate EXIF orientation fixtures (exif_o1.jpg .. exif_o8.jpg) for #1365.

Each file stores a physically transformed bitmap with an EXIF Orientation tag such
that `megi_image::decode` (which applies the tag via `megi_image::apply_orientation`)
returns the same upright 12x8 reference image for every orientation.

For upright reference U (12x8) and EXIF tag o, the stored bitmap is
S_o = apply_orientation(U, inverse_exif(o)) so that
decode(S_o, o) = apply_orientation(S_o, o) = U.

Run from the repository root:
  python3 crates/image/tests/fixtures/generate_exif.py

Requires: Pillow (PIL).
"""

from __future__ import annotations

import io
import struct
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[4]
OUT_DIR = ROOT / "crates/image/tests/fixtures/jpeg"

# Upright reference quadrants (12x8, 6x4 blocks).
TL = (255, 208, 28)
TR = (0, 255, 121)
BL = (0, 33, 255)
BR = (255, 181, 0)


def build_jpeg_with_exif(image: Image.Image, orientation: int, quality: int = 95) -> bytes:
    buf = io.BytesIO()
    image.save(buf, format="JPEG", quality=quality, subsampling=0)
    data = buf.getvalue()
    assert data[:2] == b"\xff\xd8"

    # APP1 Exif with IFD0 containing only Orientation (tag 0x0112, SHORT, 1).
    ifd0 = (
        struct.pack("<H", 1)
        + struct.pack("<HHI", 0x0112, 3, 1)
        + struct.pack("<H", orientation)
        + b"\x00\x00"
        + struct.pack("<I", 0)
    )
    tiff = b"II" + struct.pack("<H", 42) + struct.pack("<I", 8) + ifd0
    payload = b"Exif\x00\x00" + tiff
    app1 = b"\xff\xe1" + struct.pack(">H", len(payload) + 2) + payload
    return data[:2] + app1 + data[2:]


def apply_orientation_like_crate(img: Image.Image, orientation: int) -> Image.Image:
    """Mirror crates/image/src/lib.rs::apply_orientation pixel mapping exactly."""
    width, height = img.size
    src = img.load()
    if orientation in (5, 6, 7, 8):
        dst_w, dst_h = height, width
    else:
        dst_w, dst_h = width, height
    dst = Image.new("RGB", (dst_w, dst_h))
    dp = dst.load()
    for y in range(dst_h):
        for x in range(dst_w):
            if orientation == 2:
                sx, sy = (width - 1 - x, y)
            elif orientation == 3:
                sx, sy = (width - 1 - x, height - 1 - y)
            elif orientation == 4:
                sx, sy = (x, height - 1 - y)
            elif orientation == 5:
                sx, sy = (y, x)
            elif orientation == 6:
                sx, sy = (y, height - 1 - x)
            elif orientation == 7:
                sx, sy = (width - 1 - y, height - 1 - x)
            elif orientation == 8:
                sx, sy = (width - 1 - y, x)
            else:
                sx, sy = (x, y)
            dp[x, y] = src[sx, sy]
    return dst


def inverse_exif_orientation(orientation: int) -> int:
    return {1: 1, 2: 2, 3: 3, 4: 4, 5: 5, 6: 8, 7: 7, 8: 6}[orientation]


def make_reference() -> Image.Image:
    # 12x8, four 6x4 quadrants.
    img = Image.new("RGB", (12, 8))
    px = img.load()
    colors = {(0, 0): TL, (1, 0): TR, (0, 1): BL, (1, 1): BR}
    for y in range(8):
        for x in range(12):
            px[x, y] = colors[(x // 6, y // 4)]
    return img


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
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    reference = make_reference()
    for orientation in range(1, 9):
        stored = apply_orientation_like_crate(reference, inverse_exif_orientation(orientation))
        data = build_jpeg_with_exif(stored, orientation)
        path = OUT_DIR / f"exif_o{orientation}.jpg"
        path.write_bytes(data)
        print(f"wrote {path.name}  stored {stored.size[0]}x{stored.size[1]}  {len(data)} bytes")
    sync_copies(OUT_DIR, ROOT)


if __name__ == "__main__":
    main()
