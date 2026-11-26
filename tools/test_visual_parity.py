#!/usr/bin/env python3
"""Unit tests for tools/visual_parity.py (no Rust compilation required)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("visual_parity.py")
_spec = importlib.util.spec_from_file_location("visual_parity", MODULE_PATH)
visual_parity = importlib.util.module_from_spec(_spec)
sys.modules["visual_parity"] = visual_parity
_spec.loader.exec_module(visual_parity)


class CompareTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.a = self.root / "a.png"
        self.b = self.root / "b.png"

    def test_identical_images_report_zero_difference(self) -> None:
        from PIL import Image

        Image.new("RGB", (8, 4), "white").save(self.a)
        Image.new("RGB", (8, 4), "white").save(self.b)
        report = visual_parity.compare_images(self.a, self.b)
        self.assertEqual(report["changed_pixels"], 0)
        self.assertEqual(report["changed_pct"], 0.0)
        self.assertEqual(report["mean_abs_diff"], 0.0)
        self.assertIsNone(report["diff_bbox"])

    def test_one_changed_pixel_is_counted_above_threshold(self) -> None:
        from PIL import Image

        white = Image.new("RGB", (8, 4), "white")
        white.save(self.a)
        black = Image.new("RGB", (8, 4), "white")
        black.putpixel((2, 1), (0, 0, 0))
        black.save(self.b)
        report = visual_parity.compare_images(self.a, self.b, threshold=16)
        self.assertEqual(report["changed_pixels"], 1)
        self.assertEqual(report["changed_pct"], 3.12)
        self.assertEqual(report["diff_bbox"], (2, 1, 3, 2))

    def test_threshold_suppresses_small_differences(self) -> None:
        from PIL import Image

        Image.new("RGB", (4, 4), (10, 10, 10)).save(self.a)
        Image.new("RGB", (4, 4), (18, 18, 18)).save(self.b)
        self.assertEqual(visual_parity.compare_images(self.a, self.b, 16)["changed_pixels"], 0)
        self.assertEqual(visual_parity.compare_images(self.a, self.b, 4)["changed_pixels"], 16)

    def test_size_mismatch_is_a_typed_error(self) -> None:
        from PIL import Image

        Image.new("RGB", (4, 4), "white").save(self.a)
        Image.new("RGB", (8, 4), "white").save(self.b)
        with self.assertRaises(ValueError):
            visual_parity.compare_images(self.a, self.b)

    def test_negative_threshold_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            visual_parity.compare_images(Path("x"), Path("y"), -1)


class SideBySideTests(unittest.TestCase):
    def test_writes_double_width_canvas(self) -> None:
        from PIL import Image

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            a, b, out = root / "a.png", root / "b.png", root / "side.png"
            Image.new("RGB", (6, 3), "red").save(a)
            Image.new("RGB", (6, 3), "blue").save(b)
            visual_parity.side_by_side(a, b, out)
            with Image.open(out) as image:
                self.assertEqual(image.size, (12, 3))


class CliTests(unittest.TestCase):
    def test_json_output_and_side_flag(self) -> None:
        from PIL import Image

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            a, b, side = root / "a.png", root / "b.png", root / "side.png"
            Image.new("RGB", (4, 4), "white").save(a)
            Image.new("RGB", (4, 4), "white").save(b)
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), str(a), str(b), "--json", "--side", str(side)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["changed_pixels"], 0)
            self.assertTrue(side.exists())


if __name__ == "__main__":
    unittest.main()
