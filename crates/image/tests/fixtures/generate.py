#!/usr/bin/env python3
"""Generates the PNG/zlib test fixtures for the `image` crate.

This is an *independent* encoder: it uses Python's zlib + struct + binascii
(not the Rust code under test) to write PNGs with chosen colour type, bit
depth, per-row filter and interlace mode, plus the expected RGBA8 bytes.

Run from the crate root:
    python3 tests/fixtures/generate.py

It writes:
    tests/fixtures/png/*.png      PNG fixtures
    tests/fixtures/png/*.rgba     expected RGBA8 row-major bytes
    tests/data/zlib_vectors.rs    zlib/deflate vectors embedded in the Rust tests
"""

from __future__ import annotations

import binascii
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent
PNG_DIR = ROOT / "png"
CRATE = ROOT.parent.parent


def chunk(kind: bytes, data: bytes) -> bytes:
    crc = binascii.crc32(kind + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", crc)


def paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def filter_row(raw: bytes, prev: bytes, kind: int, bpp: int) -> bytes:
    out = bytearray(len(raw))
    for i, value in enumerate(raw):
        left = raw[i - bpp] if i >= bpp else 0
        up = prev[i] if prev else 0
        upper_left = prev[i - bpp] if (prev and i >= bpp) else 0
        if kind == 0:
            predictor = 0
        elif kind == 1:
            predictor = left
        elif kind == 2:
            predictor = up
        elif kind == 3:
            predictor = (left + up) // 2
        else:
            predictor = paeth(left, up, upper_left)
        out[i] = (value - predictor) & 0xFF
    return bytes(out)


ADAM7 = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]


def build_png(width, height, bit_depth, color_type, pixel, *, palette=None, trns=None, interlace=0):
    """Builds a PNG. `pixel(x, y)` returns a tuple of raw samples (ints)."""
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color_type]
    bits_per_pixel = channels * bit_depth

    def row_bytes(samples):
        if bit_depth == 8:
            return bytes(samples)
        if bit_depth == 16:
            out = bytearray()
            for s in samples:
                out += struct.pack(">H", s)
            return bytes(out)
        # 1/2/4-bit: pack MSB first
        out = bytearray()
        acc = 0
        nbits = 0
        per_byte = 8 // bit_depth
        for i, s in enumerate(samples):
            acc = (acc << bit_depth) | s
            nbits += bit_depth
            if (i + 1) % per_byte == 0:
                out.append(acc)
                acc = 0
                nbits = 0
        if nbits:
            out.append(acc << (8 - nbits))
        return bytes(out)

    def flatten(px):
        return [v for sample in px for v in (sample if isinstance(sample, tuple) else (sample,))]

    filtered = bytearray()
    if interlace == 0:
        prev = b""
        for y in range(height):
            raw = row_bytes(flatten([pixel(x, y) for x in range(width)]))
            kind = y % 5
            filtered += bytes([kind]) + filter_row(raw, prev, kind, max(1, (bits_per_pixel + 7) // 8))
            prev = raw
    else:
        for pass_index, (x0, y0, dx, dy) in enumerate(ADAM7):
            pw = (width - x0 + dx - 1) // dx if width > x0 else 0
            ph = (height - y0 + dy - 1) // dy if height > y0 else 0
            if pw == 0 or ph == 0:
                continue
            prev = b""
            for py in range(ph):
                y = y0 + py * dy
                samples = [pixel(x0 + px * dx, y) for px in range(pw)]
                raw = row_bytes(flatten(samples))
                kind = (pass_index + py) % 5
                filtered += bytes([kind]) + filter_row(raw, prev, kind, max(1, (bits_per_pixel + 7) // 8))
                prev = raw

    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, interlace))
    if palette is not None:
        data += chunk(b"PLTE", b"".join(bytes(rgb) for rgb in palette))
    if trns is not None:
        data += chunk(b"tRNS", trns)
    data += chunk(b"IDAT", zlib.compress(bytes(filtered), 6))
    data += chunk(b"IEND", b"")
    return data


def expected_rgba(width, height, bit_depth, color_type, pixel, *, palette=None, trns=None):
    """Independent expected RGBA8 conversion (mirrors the documented contract)."""
    out = bytearray()
    gray_max = (1 << bit_depth) - 1
    trns_gray = None
    trns_rgb = None
    if trns is not None and color_type == 0:
        trns_gray = struct.unpack(">H", trns)[0]
    if trns is not None and color_type == 2:
        trns_rgb = struct.unpack(">HHH", trns)
    for y in range(height):
        for x in range(width):
            s = pixel(x, y)
            if color_type == 0:
                g = s
                v = g if bit_depth == 8 else (g if bit_depth == 16 else g * 255 // gray_max)
                if bit_depth == 16:
                    v = g >> 8
                a = 0 if trns_gray is not None and g == trns_gray else 255
                out += bytes([v, v, v, a])
            elif color_type == 2:
                r, g, b = s
                if bit_depth == 16:
                    r8, g8, b8 = r >> 8, g >> 8, b >> 8
                else:
                    r8, g8, b8 = r, g, b
                a = 0 if trns_rgb is not None and (r, g, b) == trns_rgb else 255
                out += bytes([r8, g8, b8, a])
            elif color_type == 3:
                idx = s
                pr, pg, pb = palette[idx]
                a = trns[idx] if trns is not None and idx < len(trns) else 255
                out += bytes([pr, pg, pb, a])
            elif color_type == 4:
                g, a = s
                v = g >> 8 if bit_depth == 16 else g
                out += bytes([v, v, v, a >> 8 if bit_depth == 16 else a])
            else:
                r, g, b, a = s
                if bit_depth == 16:
                    out += bytes([r >> 8, g >> 8, b >> 8, a >> 8])
                else:
                    out += bytes([r, g, b, a])
    return bytes(out)


def emit(name, png, rgba):
    (PNG_DIR / f"{name}.png").write_bytes(png)
    (PNG_DIR / f"{name}.rgba").write_bytes(rgba)
    print(f"{name:<24} png={len(png):6d} B  rgba={len(rgba):6d} B")


def gray8(x, y):
    return (x * 31 + y * 17) & 0xFF


def main():
    PNG_DIR.mkdir(parents=True, exist_ok=True)

    # --- colour types, bit depths 8 and 16 -------------------------------
    emit("gray8", build_png(4, 3, 8, 0, gray8),
         expected_rgba(4, 3, 8, 0, gray8))
    emit("rgb8", build_png(5, 4, 8, 2, lambda x, y: ((x * 40) & 0xFF, (y * 60) & 0xFF, (x * y * 13) & 0xFF)),
         expected_rgba(5, 4, 8, 2, lambda x, y: ((x * 40) & 0xFF, (y * 60) & 0xFF, (x * y * 13) & 0xFF)))
    emit("rgba8", build_png(6, 5, 8, 6, lambda x, y: ((x * 37) & 0xFF, (y * 53) & 0xFF, (x + y * 11) & 0xFF, (x * y * 19) & 0xFF)),
         expected_rgba(6, 5, 8, 6, lambda x, y: ((x * 37) & 0xFF, (y * 53) & 0xFF, (x + y * 11) & 0xFF, (x * y * 19) & 0xFF)))
    emit("gray_alpha8", build_png(3, 2, 8, 4, lambda x, y: (x * 70, y * 90 + 10)),
         expected_rgba(3, 2, 8, 4, lambda x, y: (x * 70, y * 90 + 10)))
    emit("gray16", build_png(3, 2, 16, 0, lambda x, y: (x * 12345 + y * 4321) & 0xFFFF),
         expected_rgba(3, 2, 16, 0, lambda x, y: (x * 12345 + y * 4321) & 0xFFFF))
    emit("rgb16", build_png(3, 2, 16, 2, lambda x, y: ((x * 1111) & 0xFFFF, (y * 2222 + 3) & 0xFFFF, ((x + y) * 3333) & 0xFFFF)),
         expected_rgba(3, 2, 16, 2, lambda x, y: ((x * 1111) & 0xFFFF, (y * 2222 + 3) & 0xFFFF, ((x + y) * 3333) & 0xFFFF)))
    emit("rgba16", build_png(3, 2, 16, 6, lambda x, y: ((x * 1234) & 0xFFFF, (y * 2345) & 0xFFFF, (x * y * 3456) & 0xFFFF, (x + y * 4567) & 0xFFFF)),
         expected_rgba(3, 2, 16, 6, lambda x, y: ((x * 1234) & 0xFFFF, (y * 2345) & 0xFFFF, (x * y * 3456) & 0xFFFF, (x + y * 4567) & 0xFFFF)))
    emit("gray_alpha16", build_png(3, 2, 16, 4, lambda x, y: ((x * 6000 + 7) & 0xFFFF, (y * 7000 + 9) & 0xFFFF)),
         expected_rgba(3, 2, 16, 4, lambda x, y: ((x * 6000 + 7) & 0xFFFF, (y * 7000 + 9) & 0xFFFF)))

    # --- sub-byte grayscale ---------------------------------------------
    for depth in (1, 2, 4):
        w, h = 9, 3
        maxv = (1 << depth) - 1
        emit(f"gray{depth}",
             build_png(w, h, depth, 0, lambda x, y: (x + y) % (maxv + 1)),
             expected_rgba(w, h, depth, 0, lambda x, y: (x + y) % (maxv + 1)))

    # --- palette ---------------------------------------------------------
    palette7 = [(10 * i & 0xFF, 255 - 20 * i & 0xFF, 40 * i & 0xFF) for i in range(7)]
    rgba_trns = bytes([0, 128, 255])  # first three palette entries have alpha
    for depth, w in ((1, 10), (2, 6), (4, 5), (8, 4)):
        entries = min(1 << depth, len(palette7))
        pal = palette7[:entries]
        trns = rgba_trns[:entries]
        emit(f"palette{depth}",
             build_png(w, 3, depth, 3, lambda x, y: (x + 2 * y) % entries, palette=pal, trns=trns),
             expected_rgba(w, 3, depth, 3, lambda x, y: (x + 2 * y) % entries, palette=pal, trns=trns))

    # --- tRNS on gray/RGB ------------------------------------------------
    trns_gray = struct.pack(">H", 77)
    emit("trns_gray", build_png(4, 2, 8, 0, lambda x, y: 77 if (x + y) % 3 == 0 else (x * 30 + y * 5), trns=trns_gray),
         expected_rgba(4, 2, 8, 0, lambda x, y: 77 if (x + y) % 3 == 0 else (x * 30 + y * 5), trns=trns_gray))
    trns_rgb = struct.pack(">HHH", 1, 2, 3)
    emit("trns_rgb", build_png(4, 2, 8, 2, lambda x, y: (1, 2, 3) if (x + y) % 2 == 0 else (x * 60, y * 30, 99), trns=trns_rgb),
         expected_rgba(4, 2, 8, 2, lambda x, y: (1, 2, 3) if (x + y) % 2 == 0 else (x * 60, y * 30, 99), trns=trns_rgb))

    # --- per-row filters 0..4 (the generator already cycles filters) -----
    emit("filters_rgba8", build_png(7, 10, 8, 6, lambda x, y: ((x * 33 + y) & 0xFF, (y * 41 + x) & 0xFF, (x * y * 7) & 0xFF, (x * 29 + y * 13) & 0xFF)),
         expected_rgba(7, 10, 8, 6, lambda x, y: ((x * 33 + y) & 0xFF, (y * 41 + x) & 0xFF, (x * y * 7) & 0xFF, (x * 29 + y * 13) & 0xFF)))
    emit("filters_rgb16", build_png(5, 10, 16, 2, lambda x, y: ((x * 900 + y * 7) & 0xFFFF, (y * 1700 + x) & 0xFFFF, (x * y * 101) & 0xFFFF)),
         expected_rgba(5, 10, 16, 2, lambda x, y: ((x * 900 + y * 7) & 0xFFFF, (y * 1700 + x) & 0xFFFF, (x * y * 101) & 0xFFFF)))

    # --- Adam7 interlace -------------------------------------------------
    emit("interlaced_rgba8", build_png(9, 7, 8, 6, lambda x, y: ((x * 23) & 0xFF, (y * 31) & 0xFF, (x * y * 11) & 0xFF, (x * 45 + y) & 0xFF), interlace=1),
         expected_rgba(9, 7, 8, 6, lambda x, y: ((x * 23) & 0xFF, (y * 31) & 0xFF, (x * y * 11) & 0xFF, (x * 45 + y) & 0xFF)))
    emit("interlaced_rgb8", build_png(7, 5, 8, 2, lambda x, y: ((x * 51) & 0xFF, (y * 77) & 0xFF, (x * y * 3) & 0xFF), interlace=1),
         expected_rgba(7, 5, 8, 2, lambda x, y: ((x * 51) & 0xFF, (y * 77) & 0xFF, (x * y * 3) & 0xFF)))

    # --- zlib/deflate vectors for the inflate tests ----------------------
    stored_data = b"stored block payload " * 4
    fixed_data = b"hello fixed huffman " * 3
    dynamic_data = bytes(((i * 37 + (i >> 3) * 11) & 0xFF) for i in range(600)) + b"repeat-me " * 40
    window = bytes(((i * 131 + 7) & 0xFF) for i in range(32768))
    max_distance = window + window + b"tail"
    long_match = (b"0123456789abcdef" * 256) + bytes(((i * 7 + i // 7) & 0xFF) for i in range(2000)) + (b"0123456789abcdef" * 256)

    def strip_zlib(blob: bytes) -> bytes:
        return blob[2:-4]

    def hexstr(blob: bytes) -> str:
        return ", ".join(f"0x{b:02x}" for b in blob)

    vectors = {
        "STORED": zlib.compress(stored_data, 0),
        "STORED_RAW": strip_zlib(zlib.compress(stored_data, 0)),
        "FIXED": zlib.compressobj(6, zlib.DEFLATED, 15, 8, zlib.Z_FIXED).compress(fixed_data) + zlib.compressobj(6, zlib.DEFLATED, 15, 8, zlib.Z_FIXED).flush(),
        "DYNAMIC": zlib.compress(dynamic_data, 9),
        "MAX_DISTANCE": zlib.compress(max_distance, 9),
        "LONG_MATCH": zlib.compress(long_match, 9),
    }
    # FIXED built in one compressobj
    compressobj = zlib.compressobj(6, zlib.DEFLATED, 15, 8, zlib.Z_FIXED)
    vectors["FIXED"] = compressobj.compress(fixed_data) + compressobj.flush()
    vectors["FIXED_RAW"] = strip_zlib(vectors["FIXED"])
    vectors["DYNAMIC_RAW"] = strip_zlib(vectors["DYNAMIC"])
    corrupted = bytearray(zlib.compress(stored_data, 0))
    corrupted[-1] ^= 0xFF
    vectors["BAD_ADLER"] = bytes(corrupted)
    truncated = zlib.compress(dynamic_data, 9)[:40]
    vectors["TRUNCATED"] = truncated

    def indent(hex_text: str) -> str:
        parts = hex_text.split(", ")
        lines, line = [], []
        for part in parts:
            line.append(part)
            if len(line) == 12:
                lines.append("    " + ", ".join(line) + ",")
                line = []
        if line:
            lines.append("    " + ", ".join(line) + ",")
        return "\n".join(lines)

    out = ["// @generated by tests/fixtures/generate.py — do not edit by hand.", ""]
    for name, blob in vectors.items():
        out.append(f"pub const {name}: &[u8] = &[")
        out.append(indent(hexstr(blob)))
        out.append("];")
        out.append("")
    out.append("pub const STORED_DATA: &[u8] = b\"stored block payload stored block payload stored block payload stored block payload \";")
    out.append("pub const FIXED_DATA: &[u8] = b\"hello fixed huffman hello fixed huffman hello fixed huffman \";")
    out.append("")
    (CRATE / "tests" / "data" / "zlib_vectors.rs").write_text("\n".join(out))
    print("zlib vectors written to tests/data/zlib_vectors.rs")


if __name__ == "__main__":
    main()
