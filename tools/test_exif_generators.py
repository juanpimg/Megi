#!/usr/bin/env python3
"""Regression tests for the EXIF fixture generators (#1652).

Two scripts used to write the same `exif_o1..o8.jpg` names with different
geometries (12x8 vs 16x16), so whichever ran last silently replaced the fixtures
and broke the Rust tests that pin the canonical 16x16 geometry. These tests
assert there is a single canonical geometry source and that the legacy entry
point delegates to it instead of reimplementing it.
"""

from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/image/tests/fixtures"
LEGACY = FIXTURES / "generate_exif.py"
CANONICAL = FIXTURES / "generate_exif_orientations.py"


class ExifGeneratorTests(unittest.TestCase):
    def test_canonical_generator_owns_the_geometry(self) -> None:
        source = CANONICAL.read_text(encoding="utf-8")
        self.assertIn("W, H = 16, 16", source)
        self.assertIn("exif_o{orientation}.jpg", source)

    def test_legacy_generator_does_not_define_a_conflicting_geometry(self) -> None:
        source = LEGACY.read_text(encoding="utf-8")
        self.assertNotIn(
            'Image.new("RGB", (12, 8))',
            source,
            "legacy generator must not create 12x8 fixtures (#1652)",
        )
        self.assertNotIn(
            "REFERENCE_WIDTH, REFERENCE_HEIGHT",
            source,
            "legacy generator must not own a geometry constant",
        )

    def test_legacy_generator_delegates_to_canonical(self) -> None:
        source = LEGACY.read_text(encoding="utf-8")
        self.assertIn(
            "from generate_exif_orientations import",
            source,
            "legacy entry point must delegate to the single canonical generator (#1652)",
        )


if __name__ == "__main__":
    unittest.main()
