#!/usr/bin/env python3
"""Unit tests for tools/check_agents_constraints.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_agents_constraints.py")
_spec = importlib.util.spec_from_file_location("check_agents_constraints", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_agents_constraints"] = checker
_spec.loader.exec_module(checker)

AGENTS = "Reglas: 500 líneas, máximo 10 WPs, forbid(unsafe_code).\n"
ARCH = "MAX_LINES = 500\n"
PLAN = "MAX_IN_PROGRESS = 10\n"


class ParseTests(unittest.TestCase):
    def test_reads_integer_constants(self) -> None:
        self.assertEqual(checker.int_const(ARCH, "MAX_LINES"), 500)
        self.assertEqual(checker.int_const(PLAN, "MAX_IN_PROGRESS"), 10)

    def test_missing_constant_is_none(self) -> None:
        self.assertIsNone(checker.int_const("nada", "MAX_LINES"))


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "tools").mkdir()
        (self.root / "AGENTS.md").write_text(AGENTS, encoding="utf-8")
        (self.root / "tools/check_arch.py").write_text(ARCH, encoding="utf-8")
        (self.root / "tools/wp_plan.py").write_text(PLAN, encoding="utf-8")

    def test_valid_constraints_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "AGENTS.md").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_rule_is_reported(self) -> None:
        (self.root / "AGENTS.md").write_text("sin reglas\n", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("500-line" in p for p in problems))
        self.assertTrue(any("10 in_progress" in p for p in problems))
        self.assertTrue(any("forbid(unsafe_code)" in p for p in problems))

    def test_mismatched_limit_is_reported(self) -> None:
        (self.root / "tools/check_arch.py").write_text("MAX_LINES = 400\n", encoding="utf-8")
        self.assertTrue(any("MAX_LINES=400" in p for p in checker.validate(self.root)))

    def test_mismatched_wp_cap_is_reported(self) -> None:
        (self.root / "tools/wp_plan.py").write_text("MAX_IN_PROGRESS = 8\n", encoding="utf-8")
        self.assertTrue(any("MAX_IN_PROGRESS=8" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_rules(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("agent constraints consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("AGENTS CONSTRAINTS:", result.stderr)


if __name__ == "__main__":
    unittest.main()
