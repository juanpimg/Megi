#!/usr/bin/env python3
"""Tests for the compile-free window UI parity gate (WP-USER-ui-gate)."""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

from tools import check_window_ui  # noqa: F401 — RED until the checker exists

ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "tools" / "check_window_ui.py"


def _write_image(path: Path, color: tuple[int, int, int]) -> Path:
    Image.new("RGB", (32, 32), color).save(path)
    return path


def _run(*args: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(CHECKER), *(str(a) for a in args)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


class CheckWindowUiTests(unittest.TestCase):
    def test_compare_accepts_identical_images(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            a = _write_image(Path(tmp) / "a.png", (12, 34, 56))
            b = _write_image(Path(tmp) / "b.png", (12, 34, 56))
            result = _run("--compare", a, b, "--max-mae", "18")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("MAE=0.000", result.stdout)

    def test_compare_rejects_mae_above_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            a = _write_image(Path(tmp) / "a.png", (0, 0, 0))
            b = _write_image(Path(tmp) / "b.png", (255, 255, 255))
            result = _run("--compare", a, b, "--max-mae", "18")
            self.assertEqual(result.returncode, 1, result.stdout)
            self.assertIn("MAE=255.000", result.stdout)

    def test_missing_capture_fails_with_clear_message(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = _run("--check", "--captures", tmp)
            self.assertEqual(result.returncode, 1, result.stdout)
            output = result.stdout + result.stderr
            self.assertIn("missing capture", output)
            self.assertIn("hn.png", output)


if __name__ == "__main__":
    unittest.main()
