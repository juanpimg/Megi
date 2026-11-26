#!/usr/bin/env python3
"""Unit tests for tools/check_workflow_metadata.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_workflow_metadata.py")
_spec = importlib.util.spec_from_file_location("check_workflow_metadata", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_workflow_metadata"] = checker
_spec.loader.exec_module(checker)

VALID = """name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
  test:
    runs-on: ubuntu-latest
"""


class CheckFileTests(unittest.TestCase):
    def test_valid_workflow_passes(self) -> None:
        self.assertEqual(checker.check_file(VALID), [])

    def test_missing_name_is_reported(self) -> None:
        self.assertTrue(any("name:" in p for p in checker.check_file(VALID.replace("name: CI\n", ""))))

    def test_missing_on_is_reported(self) -> None:
        self.assertTrue(any("on:" in p for p in checker.check_file(VALID.replace("on: [push]\n", ""))))

    def test_missing_jobs_is_reported(self) -> None:
        self.assertTrue(any("no jobs" in p for p in checker.check_file("name: CI\non: push\n")))

    def test_duplicate_jobs_are_reported(self) -> None:
        self.assertTrue(any("duplicate job ids" in p for p in checker.check_file(VALID + "  build:\n")))

    def test_tab_indentation_is_reported(self) -> None:
        self.assertTrue(any("tab indentation" in p for p in checker.check_file(VALID + "\tstep: x\n")))


class ValidateTests(unittest.TestCase):
    def test_real_workflows_pass(self) -> None:
        root = Path(__file__).resolve().parent.parent
        self.assertEqual(checker.validate(root), [])

    def test_missing_directory_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertTrue(checker.validate(Path(tmp))[0].startswith("missing"))

    def test_empty_directory_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / ".github/workflows").mkdir(parents=True)
            self.assertEqual(checker.validate(Path(tmp)), ["no workflows found"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_workflows(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("workflow metadata OK", result.stdout)


if __name__ == "__main__":
    unittest.main()
