#!/usr/bin/env python3
"""Unit tests for tools/png_check.py."""

from __future__ import annotations

import binascii
import os
import struct
import tempfile
import unittest
import zlib

import png_check


def make_chunk(kind: bytes, payload: bytes) -> bytes:
    crc = binascii.crc32(kind + payload) & 0xFFFFFFFF
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", crc)


def make_png(
    width: int,
    height: int,
    bit_depth: int,
    color_type: int,
    interlace: int,
    raw_scanlines: bytes,
) -> bytes:
    ihdr = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, interlace)
    idat = zlib.compress(raw_scanlines)
    return (
        png_check.PNG_SIGNATURE
        + make_chunk(b"IHDR", ihdr)
        + make_chunk(b"IDAT", idat)
        + make_chunk(b"IEND", b"")
    )


class PngCheckTests(unittest.TestCase):
    def test_valid_8bit_rgba(self) -> None:
        raw = b"\x00\xff\x00\x00\xff\x00\xff\x00\xff"
        data = make_png(2, 1, 8, 6, 0, raw)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp.write(data)
            path = tmp.name
        try:
            png_check.check_file(path)
        finally:
            os.unlink(path)

    def test_valid_16bit_rgba(self) -> None:
        # 1x1 16-bit RGBA = 1 filter byte + 8 sample bytes = 9 bytes
        raw = b"\x00" + b"\x12\x34" * 4
        data = make_png(1, 1, 16, 6, 0, raw)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp.write(data)
            path = tmp.name
        try:
            png_check.check_file(path)
        finally:
            os.unlink(path)

    def test_valid_adam7_interlaced(self) -> None:
        # 2x2 RGBA (channels=4, depth=8):
        # pass 1 (0,0 step 8,8): 1x1 -> 1 + 4 = 5
        # pass 6 (1,0 step 2,2): 1x1 -> 1 + 4 = 5
        # pass 7 (0,1 step 1,2): 2x1 -> 1 + 8 = 9
        # total = 19 bytes
        expected = png_check.adam7_expected_len(2, 2, 4, 8)
        self.assertEqual(expected, 19)
        data = make_png(2, 2, 8, 6, 1, b"\x00" * expected)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp.write(data)
            path = tmp.name
        try:
            png_check.check_file(path)
        finally:
            os.unlink(path)

    def test_rejects_zlib_bomb_exceeding_expected_size(self) -> None:
        # 1x1 RGBA expects 5 bytes, but zlib payload inflates to 100 bytes
        data = make_png(1, 1, 8, 6, 0, b"\x00" * 100)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp.write(data)
            path = tmp.name
        try:
            with self.assertRaises(png_check.PngError):
                png_check.check_file(path)
        finally:
            os.unlink(path)


if __name__ == "__main__":
    unittest.main()
