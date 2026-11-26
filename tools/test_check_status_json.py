#!/usr/bin/env python3
"""Unit tests for tools/check_status_json.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_status_json.py")
_spec = importlib.util.spec_from_file_location("check_status_json", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_status_json"] = checker
_spec.loader.exec_module(checker)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.status = self.root / "docs/status"
        self.status.mkdir(parents=True)
        for name in checker.REQUIRED:
            (self.status / name).write_text('{"ok": true}', encoding="utf-8")

    def test_valid_status_directory_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_required_report_is_reported(self) -> None:
        (self.status / "wpt.json").unlink()
        problems = checker.validate(self.root)
        self.assertIn("wpt.json: required status report is missing", problems)

    def test_invalid_json_is_reported(self) -> None:
        (self.status / "coverage.json").write_text("{broken", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("coverage.json: invalid JSON" in p for p in problems))

    def test_non_object_top_level_is_reported(self) -> None:
        (self.status / "coverage.json").write_text("[]", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertIn("coverage.json: top level must be a non-empty object", problems)

    def test_non_finite_constant_is_rejected(self) -> None:
        (self.status / "wpt.json").write_text('{"x": NaN}', encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("non-finite JSON constant" in p for p in problems))

    def test_missing_directory_is_reported(self) -> None:
        problems = checker.validate(self.root / "absent")
        self.assertTrue(problems[0].startswith("missing directory"))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_status_directory(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("status JSON OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("STATUS JSON:", result.stderr)


if __name__ == "__main__":
    unittest.main()
