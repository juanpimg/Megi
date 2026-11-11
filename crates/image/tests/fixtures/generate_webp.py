#!/usr/bin/env python3
"""Generates the VP8L fixtures used by the R147 WebP tests.

Two families are produced:

- simple, hand-written VP8L streams (own synthetic content, ``v8l_*``) that exercise
  the literal/Huffman subset without relying on PIL, with their exact RGBA stored;
- **real libwebp lossless encodings** (``photo_transforms``, ``palette_real``,
  ``alpha_real``, ``palette_transform``) that use the full VP8L feature set
  (predictor/colour transforms, LZ77, colour cache, colour indexing). PIL/libwebp is
  the independent oracle: the Rust decoder is compared byte-exactly against the
  RGBA oracle stored next to each fixture (``.rgba`` raw files).

Run from the repository root: python3 crates/image/tests/fixtures/generate_webp.py
"""
import os

from PIL import Image

FIXTURES = "crates/image/tests/fixtures/webp"
RENDER = "tests/fixtures/render/assets"


class BitWriter:
    def __init__(self):
        self.bits = []

    def write(self, value, count):
        for shift in range(count):
            self.bits.append((value >> shift) & 1)

    def to_bytes(self):
        out = bytearray()
        for index in range(0, len(self.bits), 8):
            byte = 0
            for shift, bit in enumerate(self.bits[index:index + 8]):
                byte |= bit << shift
            out.append(byte)
        return bytes(out)


def simple_code(writer, symbols):
    writer.write(1, 1)  # simple code
    writer.write(len(symbols) - 1, 1)  # num_symbols - 1
    first = symbols[0]
    if first < 2:
        writer.write(0, 1)
        writer.write(first, 1)
    else:
        writer.write(1, 1)
        writer.write(first, 8)
    if len(symbols) == 2:
        writer.write(symbols[1], 8)


def encode_vp8l(width, height, pixels, alpha_used):
    """pixels: list of RGBA tuples; each channel must need at most 2 symbols."""
    writer = BitWriter()
    writer.write(width - 1, 14)
    writer.write(height - 1, 14)
    writer.write(alpha_used, 1)
    writer.write(0, 3)
    writer.write(0, 1)  # no transforms
    writer.write(0, 1)  # no colour cache
    writer.write(0, 1)  # no meta Huffman
    greens = sorted({p[1] for p in pixels})
    reds = sorted({p[0] for p in pixels})
    blues = sorted({p[2] for p in pixels})
    alphas = sorted({p[3] for p in pixels})
    for channel in (greens, reds, blues, alphas):
        assert len(channel) <= 2, "fixture channels must fit a simple Huffman code"
    simple_code(writer, greens)
    simple_code(writer, reds)
    simple_code(writer, blues)
    simple_code(writer, alphas)
    simple_code(writer, [0])  # unused distance code
    for red, green, blue, alpha in pixels:
        # Green tree is single-symbol in these fixtures (no bit), red/blue/alpha are
        # one bit each when they have two symbols.
        if len(reds) == 2:
            writer.write(0 if red == reds[0] else 1, 1)
        if len(blues) == 2:
            writer.write(0 if blue == blues[0] else 1, 1)
        if len(alphas) == 2:
            writer.write(0 if alpha == alphas[0] else 1, 1)
    payload = b"\x2f" + writer.to_bytes()
    padded = payload + (b"\x00" if len(payload) % 2 else b"")
    riff_size = 4 + 8 + len(padded)
    return b"RIFF" + riff_size.to_bytes(4, "little") + b"WEBPVP8L" + len(payload).to_bytes(4, "little") + padded


def write_fixture(name, image, directory):
    os.makedirs(directory, exist_ok=True)
    width, height = image.size
    pixels = list(image.convert("RGBA").getdata())
    data = encode_vp8l(width, height, pixels, 1)
    path = os.path.join(directory, name)
    with open(path, "wb") as handle:
        handle.write(data)
    # Validate with libwebp (PIL) as an independent encoder oracle, and store the
    # expected RGBA bytes for the Rust tests.
    decoded = Image.open(path).convert("RGBA")
    assert decoded.tobytes() == image.convert("RGBA").tobytes(), f"{name}: PIL roundtrip mismatch"
    rgba_path = os.path.splitext(path)[0] + ".rgba"
    with open(rgba_path, "wb") as handle:
        handle.write(decoded.tobytes())
    print(path, len(data), "bytes")


def make_two_color(size, left, right):
    image = Image.new("RGBA", (size, size), left)
    pixels = image.load()
    for y in range(size):
        for x in range(size):
            pixels[x, y] = left if x < size // 2 else right
    return image


def write_pil_lossless_fixture(name, image):
    """Encodes `image` with PIL/libwebp lossless and stores the PIL RGBA oracle.

    These are *real* libwebp encodings (predictor/colour transforms, LZ77, colour
    cache and colour indexing as selected by the encoder), so the Rust decoder is
    validated byte-exactly against an independent implementation.
    """
    os.makedirs(FIXTURES, exist_ok=True)
    webp_path = os.path.join(FIXTURES, name + ".webp")
    image.save(webp_path, "WEBP", lossless=True, quality=100, method=6)
    decoded = Image.open(webp_path).convert("RGBA")
    rgba_path = os.path.join(FIXTURES, name + ".rgba")
    with open(rgba_path, "wb") as handle:
        handle.write(decoded.tobytes())
    print(webp_path, os.path.getsize(webp_path), "bytes (libwebp lossless)")
    return decoded


def make_photo_transforms(width, height):
    """Smooth RGB gradients + colour bars: forces predictor/colour transforms and
    the colour cache in libwebp's encoder (no palette: > 256 colours)."""
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            red = (x * 4 + y * 2) % 256
            green = (x + y * 3) % 256
            blue = ((x * 7) ^ (y * 5)) % 256
            pixels[x, y] = (red, green, blue)
    return image


def make_palette_real(width, height):
    """Eight-colour deterministic pattern: libwebp uses colour indexing (bundled
    indices) for it."""
    colors = [
        (0, 0, 0, 255),
        (20, 60, 220, 255),
        (30, 200, 60, 255),
        (120, 30, 180, 255),
        (230, 30, 40, 255),
        (240, 120, 20, 255),
        (250, 220, 30, 255),
        (255, 255, 255, 255),
    ]
    image = Image.new("RGBA", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            pixels[x, y] = colors[(x // 4 + y // 3) % len(colors)]
    return image


def make_alpha_real(width, height):
    """RGBA gradients with varying alpha (lossless alpha is part of VP8L)."""
    image = Image.new("RGBA", (width, height))
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            alpha = (x * 255) // max(1, width - 1)
            pixels[x, y] = ((x * 3) % 256, (y * 5) % 256, (x + y) % 256, alpha)
    return image


def write_real_fixtures():
    write_pil_lossless_fixture("photo_transforms", make_photo_transforms(64, 48))
    write_pil_lossless_fixture("palette_real", make_palette_real(32, 24))
    write_pil_lossless_fixture("alpha_real", make_alpha_real(40, 32))
    # Single solid colour: libwebp emits a colour-indexing transform here too.
    write_pil_lossless_fixture("palette_transform", Image.new("RGBA", (16, 16), (200, 30, 40, 255)))


def main():
    left = (255, 0, 0, 255)
    right = (0, 0, 255, 255)
    write_fixture("v8l_two_color.webp", make_two_color(8, left, right), FIXTURES)
    transparent = (0, 0, 255, 0)
    write_fixture("v8l_alpha.webp", make_two_color(8, left, transparent), FIXTURES)
    write_fixture("v8l_two_color.webp", make_two_color(8, left, right), RENDER)
    write_fixture("v8l_alpha.webp", make_two_color(8, left, transparent), RENDER)

    # Real libwebp lossless encodings (transforms/LZ77/cache/colour indexing):
    # the Rust decoder is checked byte-exactly against the PIL RGBA oracle.
    write_real_fixtures()
    # VP8 lossy is deliberately unsupported (typed error + render warning).
    lossy_path = os.path.join(FIXTURES, "lossy.webp")
    Image.new("RGB", (16, 16), (30, 160, 90)).save(lossy_path, "WEBP", quality=80)
    print(lossy_path, os.path.getsize(lossy_path), "bytes (VP8 lossy)")
    render_lossy = os.path.join(RENDER, "webp_lossy.webp")
    Image.new("RGB", (16, 16), (30, 160, 90)).save(render_lossy, "WEBP", quality=80)
    print(render_lossy, os.path.getsize(render_lossy), "bytes (VP8 lossy)")

if __name__ == "__main__":
    main()
