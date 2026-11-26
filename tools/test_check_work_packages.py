#!/usr/bin/env python3
"""Unit tests for tools/check_work_packages.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_work_packages.py")
_spec = importlib.util.spec_from_file_location("check_work_packages", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_work_packages"] = checker
_spec.loader.exec_module(checker)

VALID = "\n".join(
    f'{key} = "value"'
    for key in checker.REQUIRED
) + '\nimpact = 5\nfrequency = 5\n'


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.queue = Path(self.tmp.name)
        self.file = self.queue / "WP-DEPTH-demo.toml"
        content = VALID.replace('id = "value"', 'id = "WP-DEPTH-demo"')
        self.file.write_text(content, encoding="utf-8")

    def test_valid_package_passes(self) -> None:
        self.assertEqual(checker.validate(self.queue), [])

    def test_missing_directory_is_reported(self) -> None:
        self.assertTrue(checker.validate(self.queue / "absent")[0].startswith("missing directory"))

    def test_empty_queue_is_reported(self) -> None:
        empty = self.queue / "empty"
        empty.mkdir()
        self.assertEqual(checker.validate(empty), ["no work packages found"])

    def test_missing_required_field_is_reported(self) -> None:
        self.file.write_text(VALID.replace('metric = "value"\n', ""), encoding="utf-8")
        problems = checker.validate(self.queue)
        self.assertIn("WP-DEPTH-demo.toml: missing or empty metric", problems)

    def test_id_must_match_filename(self) -> None:
        self.file.write_text(VALID, encoding="utf-8")
        problems = checker.validate(self.queue)
        self.assertTrue(any("does not match file name" in p for p in problems))

    def test_invalid_impact_is_reported(self) -> None:
        self.file.write_text(VALID.replace("impact = 5", "impact = 0"), encoding="utf-8")
        problems = checker.validate(self.queue)
        self.assertTrue(any("impact must be a positive integer" in p for p in problems))

    def test_invalid_toml_is_reported(self) -> None:
        self.file.write_text("id = [broken", encoding="utf-8")
        problems = checker.validate(self.queue)
        self.assertTrue(any("invalid TOML" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_queue(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("work packages OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WORK PACKAGE:", result.stderr)


if __name__ == "__main__":
    unittest.main()
