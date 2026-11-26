#!/usr/bin/env python3
"""Unit tests for tools/check_python_syntax.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_python_syntax.py")
_spec = importlib.util.spec_from_file_location("check_python_syntax", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_python_syntax"] = checker
_spec.loader.exec_module(checker)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "tools").mkdir()

    def test_valid_modules_pass(self) -> None:
        (self.root / "tools/a.py").write_text("x = 1\n", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), [])

    def test_syntax_error_is_reported(self) -> None:
        (self.root / "tools/broken.py").write_text("def (:\n", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith("broken.py:"))

    def test_missing_tools_directory_is_reported(self) -> None:
        self.assertTrue(checker.validate(self.root / "absent")[0].startswith("missing directory"))

    def test_empty_tools_directory_is_reported(self) -> None:
        self.assertEqual(checker.validate(self.root), ["no tools/*.py files found"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_tree(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("python syntax OK", result.stdout)

    def test_cli_rejects_a_broken_tree(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            (root / "tools/bad.py").write_text("if:\n", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", str(root)],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("SYNTAX:", result.stderr)


if __name__ == "__main__":
    unittest.main()
