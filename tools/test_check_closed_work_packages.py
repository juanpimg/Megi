#!/usr/bin/env python3
"""Unit tests for tools/check_closed_work_packages.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_closed_work_packages.py")
_spec = importlib.util.spec_from_file_location("check_closed_work_packages", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_closed_work_packages"] = checker
_spec.loader.exec_module(checker)

GOOD = {"WP-DEPTH-a": {"id": "WP-DEPTH-a", "metric": "depth_closed", "commit": "abc1234"}}


class ValidateTests(unittest.TestCase):
    def test_valid_entry_passes(self) -> None:
        self.assertEqual(checker.validate(GOOD, lambda commit: True), [])

    def test_empty_registry_is_reported(self) -> None:
        self.assertEqual(checker.validate({}), ["closed.toml has no closed work packages"])

    def test_missing_fields_are_reported(self) -> None:
        problems = checker.validate({"WP-X-a": {"id": "WP-X-a"}}, lambda commit: True)
        self.assertIn("WP-X-a: missing or empty metric", problems)
        self.assertIn("WP-X-a: missing or empty commit", problems)

    def test_table_id_mismatch_is_reported(self) -> None:
        problems = checker.validate(
            {"WP-X-a": {"id": "WP-X-b", "metric": "m", "commit": "abc1234"}},
            lambda commit: True,
        )
        self.assertTrue(any("does not match table key" in p for p in problems))

    def test_invalid_table_key_is_reported(self) -> None:
        problems = checker.validate(
            {"bad-key": {"id": "bad-key", "metric": "m", "commit": "abc1234"}},
            lambda commit: True,
        )
        self.assertTrue(any("invalid table key" in p for p in problems))

    def test_non_ancestor_commit_is_reported(self) -> None:
        problems = checker.validate(GOOD, lambda commit: False)
        self.assertEqual(problems, ["WP-DEPTH-a: commit abc1234 is not an ancestor of HEAD"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_registry(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("closed work packages OK", result.stdout)

    def test_cli_reports_missing_registry(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--root", "/nonexistent"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("CLOSED WP:", result.stderr)


if __name__ == "__main__":
    unittest.main()
