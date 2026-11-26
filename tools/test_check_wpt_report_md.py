#!/usr/bin/env python3
"""Unit tests for tools/check_wpt_report_md.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wpt_report_md.py")
_spec = importlib.util.spec_from_file_location("check_wpt_report_md", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wpt_report_md"] = checker
_spec.loader.exec_module(checker)

REPORT = {
    "total": 2,
    "counts": {"pass": 2, "fail": 0, "timeout": 0, "notrun": 0, "error": 0},
    "epoch": {"wpt_revision": "a" * 40, "manifest_sha256": "b" * 64},
}
MD = (
    "WPT test — 2/2 subtests pasados\n\n"
    "| Status | Subtests |\n|---|---:|\n"
    "| PASS | 2 |\n| FAIL | 0 |\n| TIMEOUT | 0 |\n| NOTRUN | 0 |\n| harness_error | 0 |\n\n"
    f"Epoch: WPT commit `{'a' * 40}` · manifest SHA-256 `{'b' * 64}`.\n"
)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "wpt.json").write_text(json.dumps(REPORT), encoding="utf-8")
        (status / "wpt.md").write_text(MD, encoding="utf-8")

    def test_valid_report_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/wpt.md").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_headline_is_reported(self) -> None:
        (self.root / "docs/status/wpt.md").write_text("sin datos", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("missing headline" in p for p in problems))

    def test_wrong_count_row_is_reported(self) -> None:
        (self.root / "docs/status/wpt.md").write_text(
            MD.replace("| PASS | 2 |", "| PASS | 1 |"), encoding="utf-8"
        )
        self.assertTrue(any("missing row PASS=2" in p for p in checker.validate(self.root)))

    def test_missing_epoch_is_reported(self) -> None:
        (self.root / "docs/status/wpt.md").write_text(
            MD.replace("b" * 64, "c" * 64), encoding="utf-8"
        )
        self.assertTrue(any("missing epoch manifest_sha256" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_report(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WPT report markdown consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WPT REPORT MD:", result.stderr)


if __name__ == "__main__":
    unittest.main()
