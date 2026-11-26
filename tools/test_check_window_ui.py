#!/usr/bin/env python3
"""Synthetic-tests for tools/check_window_ui.py (Pillow only, no browser needed).

No real Chromium references or real Megi captures are generated here: every PNG
is built in memory with Pillow and written to a TemporaryDirectory. These tests
prove the gate's arithmetic and its failure modes, not a measured UI parity.
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

import tools.check_window_ui as checker

MODULE_PATH = Path(__file__).with_name("check_window_ui.py")


def solid(path: Path, size: tuple[int, int], rgb: tuple[int, int, int]) -> Path:
    """Write a deterministic synthetic PNG and return its path."""
    Image.new("RGB", size, rgb).save(path, format="PNG")
    return path


def run_cli(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(MODULE_PATH), *args],
        capture_output=True,
        text=True,
    )


class MaeTests(unittest.TestCase):
    def test_identical_images_have_zero_mae(self) -> None:
        image = Image.new("RGB", (8, 6), (10, 20, 30))
        self.assertEqual(checker.mae(image, image.copy()), 0.0)

    def test_mae_averages_the_three_channel_means(self) -> None:
        left = Image.new("RGB", (4, 4), (0, 0, 0))
        right = Image.new("RGB", (4, 4), (30, 60, 90))
        self.assertAlmostEqual(checker.mae(left, right), 60.0)


class CompareCliTests(unittest.TestCase):
    def test_identical_images_pass_with_max_mae_zero(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = solid(Path(tmp) / "a.png", (6, 6), (12, 34, 56))
            result = run_cli("--compare", str(path), str(path), "--max-mae", "0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("MAE=0.000", result.stdout)

    def test_mae_above_threshold_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            left = solid(Path(tmp) / "a.png", (6, 6), (0, 0, 0))
            right = solid(Path(tmp) / "b.png", (6, 6), (19, 19, 19))
            result = run_cli("--compare", str(left), str(right), "--max-mae", "18")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("MAE=19.000", result.stdout)
        self.assertIn("above threshold", result.stdout + result.stderr)

    def test_different_sizes_fail_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            left = solid(Path(tmp) / "a.png", (6, 6), (0, 0, 0))
            right = solid(Path(tmp) / "b.png", (7, 6), (0, 0, 0))
            result = run_cli("--compare", str(left), str(right), "--max-mae", "18")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("size mismatch", result.stderr)

    def test_invalid_png_fails_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "bad.png"
            bad.write_text("not a PNG", encoding="utf-8")
            good = solid(Path(tmp) / "good.png", (4, 4), (1, 2, 3))
            result = run_cli("--compare", str(bad), str(good), "--max-mae", "18")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("invalid PNG", result.stderr)

    def test_missing_file_fails_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            good = solid(Path(tmp) / "good.png", (4, 4), (1, 2, 3))
            missing = Path(tmp) / "missing.png"
            result = run_cli("--compare", str(missing), str(good), "--max-mae", "18")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("missing file", result.stderr)
        self.assertIn("missing.png", result.stderr)


class CheckCliTests(unittest.TestCase):
    def make_inputs(
        self, root: Path, deltas: dict[str, int]
    ) -> tuple[Path, Path]:
        references = root / "references"
        captures = root / "captures"
        references.mkdir()
        captures.mkdir()
        for site in checker.THRESHOLDS:
            solid(references / f"{site}.png", (10, 10), (0, 0, 0))
            delta = deltas[site]
            solid(captures / f"{site}.png", (10, 10), (delta, delta, delta))
        return references, captures

    def test_check_accepts_deltas_at_each_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            references, captures = self.make_inputs(
                Path(tmp), {"hn": 18, "rust": 14, "wikipedia": 22}
            )
            result = run_cli(
                "--check", "--references", str(references), "--captures", str(captures)
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for site, threshold in checker.THRESHOLDS.items():
            self.assertIn(f"{site} MAE={threshold}.000", result.stdout)

    def test_check_rejects_delta_above_one_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            references, captures = self.make_inputs(
                Path(tmp), {"hn": 18, "rust": 15, "wikipedia": 22}
            )
            result = run_cli(
                "--check", "--references", str(references), "--captures", str(captures)
            )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("rust MAE=15.000 limit=14", result.stdout)
        self.assertIn("rust: MAE 15.000 above threshold 14", result.stderr)

    def test_missing_capture_fails_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            references, captures = self.make_inputs(
                Path(tmp), {"hn": 0, "rust": 0, "wikipedia": 0}
            )
            (captures / "rust.png").unlink()
            result = run_cli(
                "--check", "--references", str(references), "--captures", str(captures)
            )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("rust: missing capture", result.stderr)
        self.assertIn("capture_window_ui.sh", result.stderr)

    def test_missing_reference_fails_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            references, captures = self.make_inputs(
                Path(tmp), {"hn": 0, "rust": 0, "wikipedia": 0}
            )
            (references / "wikipedia.png").unlink()
            result = run_cli(
                "--check", "--references", str(references), "--captures", str(captures)
            )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("wikipedia: missing reference", result.stderr)


if __name__ == "__main__":
    unittest.main()
