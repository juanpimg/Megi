#!/usr/bin/env python3
"""Validate a PNG file using only the Python standard library.

Checks performed (PNG spec: RFC 2083 / W3C PNG Second Edition):

* 8-byte PNG signature;
* chunk layout: length / type / payload / CRC-32 (IEEE, via ``binascii``);
* ``IHDR`` is the first chunk, appears exactly once, and its fields are
  consistent (bit depth vs. color type, compression/filter/interlace methods);
* ``IDAT`` chunks are contiguous and their concatenation is a valid zlib
  stream that inflates to exactly ``height * (1 + stride)`` bytes;
* every scanline filter byte is a valid filter type (0..=4);
* ``IEND`` is the last chunk and no bytes follow it.

For 8-bit images it also unfilters the scanlines and reports basic canvas
statistics (dimensions, distinct colors, transparent pixels), which is handy
to confirm that the Phase 4 pipeline painted an opaque canvas instead of
leaving the 88.7 % transparent viewport that motivated finding F-08.

Usage:
    python3 tools/png_check.py <file.png> [<file2.png> ...]

Exit status: 0 if every file is valid, 1 otherwise.
"""

from __future__ import annotations

import binascii
import struct
import sys
import zlib

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_INPUT_BYTES = 64 * 1024 * 1024
MAX_DECOMPRESSED_BYTES = 256 * 1024 * 1024

# Channels per pixel for each PNG color type (bit depth independent).
CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
COLOR_TYPE_NAMES = {
    0: "grayscale",
    2: "RGB",
    3: "indexed",
    4: "grayscale+alpha",
    6: "RGBA",
}
# Valid bit depths per color type (PNG spec §11.2.2).
VALID_BIT_DEPTHS = {
    0: (1, 2, 4, 8, 16),
    2: (8, 16),
    3: (1, 2, 4, 8),
    4: (8, 16),
    6: (8, 16),
}


class PngError(Exception):
    """Raised when the file is not a valid PNG."""


def parse_chunks(data: bytes) -> list[tuple[bytes, bytes]]:
    """Splits a PNG stream into ``(type, payload)`` chunks, verifying CRCs."""
    if not data.startswith(PNG_SIGNATURE):
        raise PngError("missing PNG signature (\\x89PNG\\r\\n\\x1a\\n)")
    if len(data) < len(PNG_SIGNATURE) + 12:
        raise PngError("file too short to contain a chunk")

    chunks: list[tuple[bytes, bytes]] = []
    pos = len(PNG_SIGNATURE)
    while True:
        if pos + 8 > len(data):
            raise PngError(f"truncated chunk header at offset {pos}")
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        chunk_type = data[pos + 4 : pos + 8]
        if not all(65 <= byte <= 90 or 97 <= byte <= 122 for byte in chunk_type):
            raise PngError(f"invalid chunk type {chunk_type!r} at offset {pos}")
        payload_start = pos + 8
        payload_end = payload_start + length
        if payload_end + 4 > len(data):
            raise PngError(f"truncated chunk {chunk_type!r} payload")
        payload = data[payload_start:payload_end]
        stored_crc = struct.unpack(">I", data[payload_end : payload_end + 4])[0]
        computed_crc = binascii.crc32(chunk_type + payload) & 0xFFFFFFFF
        if stored_crc != computed_crc:
            raise PngError(
                f"bad CRC for chunk {chunk_type!r}: "
                f"stored 0x{stored_crc:08X}, computed 0x{computed_crc:08X}"
            )
        chunks.append((chunk_type, payload))
        pos = payload_end + 4
        if chunk_type == b"IEND":
            break
        if len(chunks) > 10_000:
            raise PngError("too many chunks (no IEND?)")

    if pos != len(data):
        raise PngError(f"{len(data) - pos} trailing byte(s) after IEND")
    return chunks


def validate_header(chunks: list[tuple[bytes, bytes]]) -> tuple[int, int, int, int, int]:
    """Validates the chunk order and IHDR fields; returns (w, h, depth, color, interlace)."""
    types = [chunk_type for chunk_type, _ in chunks]
    if types[0] != b"IHDR":
        raise PngError("IHDR is not the first chunk")
    if types.count(b"IHDR") != 1:
        raise PngError("IHDR appears more than once")
    if types[-1] != b"IEND":
        raise PngError("IEND is not the last chunk")

    ihdr = chunks[0][1]
    if len(ihdr) != 13:
        raise PngError(f"IHDR payload must be 13 bytes, got {len(ihdr)}")
    width, height, bit_depth, color_type, compression, filter_method, interlace = (
        struct.unpack(">IIBBBBB", ihdr)
    )
    if width == 0 or height == 0:
        raise PngError("IHDR declares a zero dimension")
    if color_type not in CHANNELS:
        raise PngError(f"invalid color type {color_type}")
    if bit_depth not in VALID_BIT_DEPTHS[color_type]:
        raise PngError(f"bit depth {bit_depth} invalid for color type {color_type}")
    if compression != 0:
        raise PngError(f"unknown compression method {compression}")
    if filter_method != 0:
        raise PngError(f"unknown filter method {filter_method}")
    if interlace not in (0, 1):
        raise PngError(f"unknown interlace method {interlace}")
    if b"PLTE" in types and color_type not in (2, 3, 6):
        raise PngError("PLTE chunk in a type that forbids it")
    if color_type == 3 and b"PLTE" not in types:
        raise PngError("indexed PNG without PLTE chunk")

    idat_positions = [index for index, kind in enumerate(types) if kind == b"IDAT"]
    if not idat_positions:
        raise PngError("no IDAT chunk")
    if idat_positions != list(range(idat_positions[0], idat_positions[-1] + 1)):
        raise PngError("IDAT chunks are not contiguous")

    return width, height, bit_depth, color_type, interlace


def adam7_expected_len(width: int, height: int, channels: int, bit_depth: int) -> int:
    """Computes the expected inflated byte length for an Adam7-interlaced PNG."""
    passes = (
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    )
    total = 0
    for x0, y0, x_step, y_step in passes:
        if width <= x0 or height <= y0:
            continue
        pw = (width - x0 + x_step - 1) // x_step
        ph = (height - y0 + y_step - 1) // y_step
        stride = (pw * channels * bit_depth + 7) // 8
        total += ph * (stride + 1)
    return total


def paeth_predictor(left: int, up: int, up_left: int) -> int:
    estimate = left + up - up_left
    distance_left = abs(estimate - left)
    distance_up = abs(estimate - up)
    distance_up_left = abs(estimate - up_left)
    if distance_left <= distance_up and distance_left <= distance_up_left:
        return left
    if distance_up <= distance_up_left:
        return up
    return up_left


def unfilter(raw: bytes, width: int, height: int, channels: int) -> list[bytes]:
    """Reverses the PNG scanline filters, returning one bytes row per line."""
    stride = width * channels
    rows: list[bytes] = []
    previous = bytearray(stride)
    pos = 0
    for row_index in range(height):
        filter_type = raw[pos]
        pos += 1
        row = bytearray(raw[pos : pos + stride])
        pos += stride
        if filter_type == 0:  # None
            pass
        elif filter_type == 1:  # Sub
            for i in range(channels, stride):
                row[i] = (row[i] + row[i - channels]) & 0xFF
        elif filter_type == 2:  # Up
            for i in range(stride):
                row[i] = (row[i] + previous[i]) & 0xFF
        elif filter_type == 3:  # Average
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                row[i] = (row[i] + ((left + previous[i]) >> 1)) & 0xFF
        elif filter_type == 4:  # Paeth
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                up_left = previous[i - channels] if i >= channels else 0
                row[i] = (row[i] + paeth_predictor(left, previous[i], up_left)) & 0xFF
        else:
            raise PngError(f"invalid filter type {filter_type} on row {row_index}")
        rows.append(bytes(row))
        previous = row
    if pos != len(raw):
        raise PngError("zlib stream length does not match the image geometry")
    return rows


def describe_pixels(
    rows: list[bytes], bit_depth: int, color_type: int
) -> tuple[int, int, int]:
    """Returns (distinct_color_count, transparent_pixel_count, white_pixel_count)."""
    channels = CHANNELS[color_type]
    pixel_bytes = channels * bit_depth // 8
    distinct: set[int] = set()
    transparent = 0
    white = 0
    for row in rows:
        for offset in range(0, len(row), pixel_bytes):
            pixel = row[offset : offset + pixel_bytes]
            distinct.add(int.from_bytes(pixel, "big"))
            if color_type in (4, 6):
                if bit_depth == 8:
                    alpha = pixel[-1]
                else:
                    alpha = int.from_bytes(pixel[-2:], "big")
                if alpha == 0:
                    transparent += 1
                    continue
            if color_type in (2, 6) and bit_depth == 8:
                if pixel[0] == 255 and pixel[1] == 255 and pixel[2] == 255:
                    white += 1
    return len(distinct), transparent, white


def check_file(path: str) -> None:
    with open(path, "rb") as handle:
        data = handle.read(MAX_INPUT_BYTES + 1)
    if len(data) > MAX_INPUT_BYTES:
        raise PngError(f"input exceeds {MAX_INPUT_BYTES} byte limit")

    chunks = parse_chunks(data)
    width, height, bit_depth, color_type, interlace = validate_header(chunks)

    channels = CHANNELS[color_type]
    if interlace == 1:
        expected_len = adam7_expected_len(width, height, channels, bit_depth)
    else:
        stride = (width * channels * bit_depth + 7) // 8
        expected_len = height * (stride + 1)

    if expected_len > MAX_DECOMPRESSED_BYTES:
        raise PngError(
            f"declared image size ({expected_len} bytes) exceeds {MAX_DECOMPRESSED_BYTES} byte limit"
        )

    idat = b"".join(payload for kind, payload in chunks if kind == b"IDAT")
    try:
        decomp = zlib.decompressobj()
        raw = decomp.decompress(idat, expected_len + 1)
        if not decomp.eof or len(raw) > expected_len or decomp.unconsumed_tail:
            raise PngError(
                f"inflated size exceeds expected {expected_len} bytes (possible zlib bomb)"
            )
        raw += decomp.flush()
    except zlib.error as error:
        raise PngError(f"IDAT is not a valid zlib stream: {error}") from error

    if len(raw) != expected_len:
        raise PngError(
            f"inflated size mismatch: expected {expected_len} bytes, got {len(raw)}"
        )

    chunk_names = " ".join(kind.decode("ascii") for kind, _ in chunks)
    print(f"OK: {path}")
    print(
        f"  IHDR: {width}x{height}, bit_depth={bit_depth}, "
        f"color_type={color_type} ({COLOR_TYPE_NAMES[color_type]}), interlace={interlace}"
    )
    print(f"  chunks: {chunk_names}")
    print(f"  IDAT: zlib ok, {len(idat)} compressed -> {len(raw)} raw bytes")

    if interlace == 0 and bit_depth == 8:
        rows = unfilter(raw, width, height, channels)
        distinct, transparent, white = describe_pixels(rows, bit_depth, color_type)
        total = width * height
        print(
            f"  pixels: {total}, distinct_colors={distinct}, "
            f"transparent={transparent} ({100.0 * transparent / total:.2f}%), "
            f"white={white} ({100.0 * white / total:.2f}%)"
        )
    elif interlace == 0:
        stride = (width * channels * bit_depth + 7) // 8
        print(f"  filters: {len(set(raw[:: stride + 1]))} type(s)")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(f"usage: {argv[0]} <file.png> [<file2.png> ...]", file=sys.stderr)
        return 2

    failures = 0
    for path in argv[1:]:
        try:
            check_file(path)
        except (OSError, PngError) as error:
            print(f"FAIL: {path}: {error}", file=sys.stderr)
            failures += 1
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
