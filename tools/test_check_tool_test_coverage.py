#!/usr/bin/env python3
"""Unit tests for tools/check_tool_test_coverage.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_tool_test_coverage.py")
_spec = importlib.util.spec_from_file_location("check_tool_test_coverage", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_tool_test_coverage"] = checker
_spec.loader.exec_module(checker)


class DiscoverTests(unittest.TestCase):
    def test_finds_python_modules_in_tools(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            (root / "tools/a.py").write_text("", encoding="utf-8")
            (root / "tools/test_a.py").write_text("", encoding="utf-8")
            self.assertEqual(checker.discover_tools(root), {"a.py", "test_a.py"})

    def test_missing_tools_dir_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(checker.discover_tools(Path(tmp)), set())


class ValidateTests(unittest.TestCase):
    def test_test_modules_are_self_covering(self) -> None:
        self.assertEqual(checker.validate({"test_a.py"}, {}), [])

    def test_matching_test_module_is_accepted(self) -> None:
        self.assertEqual(checker.validate({"a.py", "test_a.py"}, {}), [])

    def test_documented_exemption_is_accepted(self) -> None:
        self.assertEqual(
            checker.validate({"a.py"}, {"a.py": "one-shot utility"}), []
        )

    def test_uncovered_tool_is_reported(self) -> None:
        self.assertEqual(
            checker.validate({"a.py"}, {}),
            ["a.py: no test_a.py and no documented exemption"],
        )

    def test_stale_exemption_is_reported(self) -> None:
        self.assertEqual(
            checker.validate({"a.py"}, {"gone.py": "why"}),
            [
                "a.py: no test_a.py and no documented exemption",
                "gone.py: exemption is stale (tool does not exist)",
            ],
        )

    def test_empty_tool_set_is_reported(self) -> None:
        self.assertEqual(checker.validate(set(), {}), ["no tools/*.py files found"])

    def test_empty_reason_is_reported(self) -> None:
        self.assertEqual(
            checker.validate({"a.py"}, {"a.py": "  "}),
            ["a.py: exemption has an empty reason"],
        )


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_tree(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("tool test coverage OK", result.stdout)

    def test_cli_reports_unreadable_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--manifest", str(Path(tmp) / "missing.toml")],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("check_tool_test_coverage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
