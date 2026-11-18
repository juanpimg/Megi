#!/usr/bin/env python3
"""Generate physically rotated EXIF-orientation fixtures (#1365).

The previous `exif_o1..o8.jpg` fixtures all stored an upright 12x8 frame and only
changed the orientation tag, so the real-world case (pixels stored rotated, tag says
rotate them back) was never exercised. Here each file stores the source rotated by the
*inverse* of the tag's transform, per `image::apply_orientation`:

  dst(x, y) = src(sx, sy)   with   dst == upright

Requires python3 + Pillow. Run from the repository root:
    python3 crates/image/tests/fixtures/generate_exif_orientations.py
"""

from PIL import Image

FIXTURES = "crates/image/tests/fixtures/jpeg"
W, H = 16, 16  # MCU-aligned and 4:4:4 so rotation preserves chroma alignment
COLORS = [
    (255, 208, 28),  # top-left
    (0, 255, 121),   # top-right
    (0, 33, 255),    # bottom-left
    (255, 181, 0),   # bottom-right
]


def upright_pixel(x: int, y: int) -> tuple[int, int, int]:
    return COLORS[(x >= W // 2) + (y >= H // 2) * 2]


def stored_pixel(orientation: int, sx: int, sy: int) -> tuple[int, int, int]:
    """Pixel at (sx, sy) of the stored frame so `apply_orientation` yields upright."""
    if orientation == 1:
        return upright_pixel(sx, sy)
    if orientation == 2:
        return upright_pixel(W - 1 - sx, sy)
    if orientation == 3:
        return upright_pixel(W - 1 - sx, H - 1 - sy)
    if orientation == 4:
        return upright_pixel(sx, H - 1 - sy)
    if orientation == 5:
        # src(sx, sy) = upright(sy, sx); stored is transposed (H x W).
        return upright_pixel(sy, sx)
    if orientation == 6:
        return upright_pixel(H - 1 - sy, sx)
    if orientation == 7:
        return upright_pixel(H - 1 - sy, W - 1 - sx)
    if orientation == 8:
        return upright_pixel(sy, W - 1 - sx)
    raise ValueError(orientation)


def main() -> None:
    for orientation in range(1, 9):
        sw, sh = (H, W) if orientation >= 5 else (W, H)
        stored = Image.new("RGB", (sw, sh))
        pixels = stored.load()
        for y in range(sh):
            for x in range(sw):
                pixels[x, y] = stored_pixel(orientation, x, y)
        exif = Image.Exif()
        exif[274] = orientation  # Orientation IFD tag
        path = f"{FIXTURES}/exif_o{orientation}.jpg"
        stored.save(path, quality=95, subsampling=0, exif=exif, optimize=False)
        print(f"wrote {path} ({sw}x{sh}, tag {orientation})")


if __name__ == "__main__":
    main()
