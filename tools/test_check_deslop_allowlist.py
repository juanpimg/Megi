#!/usr/bin/env python3
"""Unit tests for tools/check_deslop_allowlist.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_deslop_allowlist.py")
_spec = importlib.util.spec_from_file_location("check_deslop_allowlist", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_deslop_allowlist"] = checker
_spec.loader.exec_module(checker)

VALID = """# comment

field-leak megi_core::Document::field
module-cycle megi_core::a,megi_core::b
"""


class ValidateTests(unittest.TestCase):
    def test_valid_allowlist_passes(self) -> None:
        self.assertEqual(checker.validate(VALID), [])

    def test_malformed_line_is_reported(self) -> None:
        problems = checker.validate(VALID + "orphan\n")
        self.assertTrue(any("expected '<rule> <key>'" in p for p in problems))

    def test_unknown_rule_is_reported(self) -> None:
        problems = checker.validate(VALID + "mystery key\n")
        self.assertTrue(any("unknown rule 'mystery'" in p for p in problems))

    def test_rule_without_key_is_reported(self) -> None:
        problems = checker.validate(VALID + "tangle\n")
        self.assertTrue(any("expected '<rule> <key>'" in p for p in problems))

    def test_duplicate_entry_is_reported(self) -> None:
        problems = checker.validate(VALID + "field-leak megi_core::Document::field\n")
        self.assertTrue(any("duplicate of line 3" in p for p in problems))

    def test_empty_allowlist_is_reported(self) -> None:
        self.assertEqual(checker.validate("# only comments\n"), ["no allowlist entries found"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_allowlist(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("deslop allowlist format OK", result.stdout)

    def test_cli_reports_unreadable_file(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--allow", "DOES-NOT-EXIST"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("check_deslop_allowlist:", result.stderr)


if __name__ == "__main__":
    unittest.main()
