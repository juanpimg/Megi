#!/usr/bin/env python3
"""Regression tests for the stale image docs (issue #1662).

The crate docs must describe the contract the code actually implements:
progressive JPEG and VP8X WebP decode, nearest-neighbour PNG scaling and an
LSB-first GIF LZW bit reader. These are source-inspection tests so they run
without PIL or the decoder fixtures.
"""

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def source(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


class ImageDocsTests(unittest.TestCase):
    def test_lib_docs_do_not_claim_progressive_is_a_typed_error(self):
        docs = source("crates/image/src/lib.rs")
        self.assertNotIn(
            "typed errors for progressive",
            docs,
            "progressive JPEG decodes; docs must not call it a typed error (#1662)",
        )

    def test_lib_docs_do_not_claim_vp8x_is_unsupported(self):
        docs = source("crates/image/src/lib.rs")
        self.assertNotIn(
            "VP8X alpha/animation containers stay unsupported",
            docs,
            "static VP8X containers decode; docs are stale (#1662)",
        )
        self.assertNotIn(
            "animation/alpha-only\n//! WebP containers (VP8X)",
            docs,
        )

    def test_png_docs_do_not_claim_rgb_sums_for_scaled_path(self):
        docs = source("crates/image/src/png/decode.rs")
        self.assertNotIn(
            "accumulate per-output-pixel RGBA sums",
            docs,
            "scaled PNG uses nearest-neighbour sampling, not sums (#1662)",
        )

    def test_gif_docs_do_not_claim_msb_first_lzw(self):
        docs = source("crates/image/src/gif.rs")
        self.assertNotIn(
            "MSB-first bit reader for the LZW stream",
            docs,
            "GIF LZW codes are read least-significant-bit first (#1662)",
        )


if __name__ == "__main__":
    unittest.main()
