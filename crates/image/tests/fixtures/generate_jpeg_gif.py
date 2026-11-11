#!/usr/bin/env python3
"""Generates the synthetic JPEG/GIF/PNG fixtures used by R133 tests.

Run from the repository root (`python3 crates/image/tests/fixtures/generate_jpeg_gif.py`).
Requires python3 + Pillow; all images are synthetic (no external content) and the output is
deterministic for a given Pillow version.
"""
import os

from PIL import Image

IMAGE_FIXTURES = "crates/image/tests/fixtures"
RENDER_ASSETS = "tests/fixtures/render/assets"


def quadrant_photo(size, quadrant_colors):
    image = Image.new("RGB", (size, size))
    pixels = image.load()
    half = size // 2
    for y in range(size):
        for x in range(size):
            pixels[x, y] = quadrant_colors[(x // half, y // half)]
    return image


def write_image_fixtures():
    os.makedirs(f"{IMAGE_FIXTURES}/jpeg", exist_ok=True)
    os.makedirs(f"{IMAGE_FIXTURES}/gif", exist_ok=True)
    colors = {
        (0, 0): (220, 30, 30),
        (1, 0): (30, 200, 30),
        (0, 1): (30, 30, 220),
        (1, 1): (240, 220, 30),
    }
    photo = quadrant_photo(32, colors)
    for name, subsampling in [
        ("baseline_444.jpg", 0),
        ("baseline_422.jpg", 1),
        ("baseline_420.jpg", 2),
    ]:
        photo.save(f"{IMAGE_FIXTURES}/jpeg/{name}", "JPEG", quality=95, subsampling=subsampling)
    progressive = Image.new("RGB", (12, 8))
    progressive_pixels = progressive.load()
    for y in range(8):
        for x in range(12):
            progressive_pixels[x, y] = (x * 20 % 256, y * 30 % 256, (x + y) * 15 % 256)
    progressive.save(
        f"{IMAGE_FIXTURES}/jpeg/progressive.jpg", "JPEG", quality=90, progressive=True
    )
    data = open(f"{IMAGE_FIXTURES}/jpeg/baseline_420.jpg", "rb").read()
    open(f"{IMAGE_FIXTURES}/jpeg/corrupt.jpg", "wb").write(data[: len(data) // 3])

    gray = Image.new("L", (16, 12))
    pixels = gray.load()
    for y in range(12):
        for x in range(16):
            pixels[x, y] = x * 8 + y * 4
    gray.save(f"{IMAGE_FIXTURES}/jpeg/grayscale.jpg", "JPEG", quality=95)

    palette = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0] + [0, 0, 0] * 252
    blocks = Image.new("P", (20, 12))
    blocks.putpalette(palette)
    pixels = blocks.load()
    for y in range(12):
        for x in range(20):
            pixels[x, y] = (x // 5) % 4
    blocks.save(f"{IMAGE_FIXTURES}/gif/palette.gif")

    transparent = Image.new("P", (20, 12), 0)
    transparent.putpalette(palette)
    pixels = transparent.load()
    for y in range(6):
        for x in range(8):
            pixels[4 + x, 3 + y] = (x // 2 + 1) % 4
    transparent.save(f"{IMAGE_FIXTURES}/gif/transparent.gif", transparency=0)

    frames = []
    for index in (1, 2):
        frame = Image.new("P", (12, 8), 0)
        frame.putpalette(palette)
        pixels = frame.load()
        for y in range(8):
            for x in range(12):
                pixels[x, y] = index
        frames.append(frame)
    frames[0].save(
        f"{IMAGE_FIXTURES}/gif/animated.gif",
        save_all=True,
        append_images=frames[1:],
        duration=100,
        loop=0,
    )

    big = Image.new("RGB", (800, 600))
    pixels = big.load()
    for y in range(600):
        for x in range(800):
            pixels[x, y] = (x * 255 // 800, y * 255 // 600, (x + y) * 255 // 1400)
    big.save(f"{IMAGE_FIXTURES}/png/large_800x600.png", optimize=False)


def _smooth_photo(width, height):
    """Fully smooth RGB gradients (no modulo discontinuities) so the reduced-IDCT
    oracle (PIL/libjpeg draft) and the Rust decoder agree within a few units."""
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            pixels[x, y] = (
                x * 255 // (width - 1),
                y * 255 // (height - 1),
                (x + y) * 255 // (width + height - 2),
            )
    return image


def _build_exif_thumbnail(base_path, thumbnail_bytes):
    """Inserts an APP1/Exif segment with IFD0 (orientation 1) and an IFD1 JPEG
    thumbnail (tags 0x0201/0x0202) right after SOI. Deterministic little-endian TIFF."""
    import struct

    base = open(base_path, "rb").read()
    assert base[:2] == b"\xff\xd8"
    ifd0 = (
        struct.pack("<H", 1)
        + struct.pack("<HHI4s", 0x0112, 3, 1, struct.pack("<H", 1) + b"\x00\x00")
        + struct.pack("<I", 26)
    )
    ifd1_offset = 26
    thumbnail_offset = ifd1_offset + 2 + 2 * 12 + 4
    ifd1 = (
        struct.pack("<H", 2)
        + struct.pack("<HHII", 0x0201, 4, 1, thumbnail_offset)
        + struct.pack("<HHII", 0x0202, 4, 1, len(thumbnail_bytes))
        + struct.pack("<I", 0)
    )
    tiff = b"II" + struct.pack("<H", 42) + struct.pack("<I", 8) + ifd0 + ifd1 + thumbnail_bytes
    payload = b"Exif\x00\x00" + tiff
    app1 = b"\xff\xe1" + struct.pack(">H", len(payload) + 2) + payload
    open(base_path, "wb").write(base[:2] + app1 + base[2:])


def write_reduced_and_thumbnail_fixtures():
    """R178 fixtures: 1024x768 4:4:4 JPEG + PIL-draft reduced oracles, and a JPEG
    carrying an EXIF IFD1 thumbnail (the `exif_thumbnail` API and scaled-decode policy)."""
    import io

    jpeg_dir = f"{IMAGE_FIXTURES}/jpeg"
    os.makedirs(jpeg_dir, exist_ok=True)
    photo = _smooth_photo(1024, 768)
    reduced = f"{jpeg_dir}/reduced_1024x768.jpg"
    photo.save(reduced, "JPEG", quality=85, subsampling=0)
    with open(f"{jpeg_dir}/reduced_1024x768.rgba", "wb") as handle:
        handle.write(Image.open(reduced).convert("RGBA").tobytes())
    for scale in (2, 4, 8):
        draft = Image.open(reduced)
        draft.draft("RGB", (1024 // scale, 768 // scale))
        with open(f"{jpeg_dir}/reduced_1024x768_s{scale}.rgba", "wb") as handle:
            handle.write(draft.convert("RGBA").tobytes())

    base = f"{jpeg_dir}/exif_thumb.jpg"
    _smooth_photo(320, 240).save(base, "JPEG", quality=85, subsampling=0)
    thumbnail = _smooth_photo(160, 120)
    buffer = io.BytesIO()
    thumbnail.save(buffer, "JPEG", quality=85, subsampling=0)
    thumbnail_bytes = buffer.getvalue()
    _build_exif_thumbnail(base, thumbnail_bytes)
    with open(f"{jpeg_dir}/exif_thumb.rgba", "wb") as handle:
        handle.write(Image.open(base).convert("RGBA").tobytes())
    with open(f"{jpeg_dir}/exif_thumb_thumbnail.rgba", "wb") as handle:
        handle.write(Image.open(io.BytesIO(thumbnail_bytes)).convert("RGBA").tobytes())


def write_render_assets():
    os.makedirs(RENDER_ASSETS, exist_ok=True)
    colors = {
        (0, 0): (220, 30, 30),
        (1, 0): (30, 200, 30),
        (0, 1): (30, 30, 220),
        (1, 1): (240, 220, 30),
    }
    quadrant_photo(64, colors).save(f"{RENDER_ASSETS}/photo.jpg", "JPEG", quality=95, subsampling=2)
    data = open(f"{RENDER_ASSETS}/photo.jpg", "rb").read()
    open(f"{RENDER_ASSETS}/corrupt.jpg", "wb").write(data[: len(data) // 3])

    palette = [220, 30, 30, 30, 200, 30, 30, 30, 220, 240, 220, 30] + [0, 0, 0] * 252
    blocks = Image.new("P", (32, 32))
    blocks.putpalette(palette)
    pixels = blocks.load()
    for y in range(32):
        for x in range(32):
            pixels[x, y] = (x // 16) + (y // 16) * 2
    blocks.save(f"{RENDER_ASSETS}/blocks.gif")

    Image.new("RGB", (32, 32), (20, 140, 160)).save(
        f"{RENDER_ASSETS}/bg_photo.jpg", "JPEG", quality=95
    )


if __name__ == "__main__":
    write_image_fixtures()
    write_reduced_and_thumbnail_fixtures()
    write_render_assets()
    print("JPEG/GIF/PNG fixtures regenerated")
