#!/usr/bin/env python3
"""Unit tests for tools/check_agent_index_refs.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_agent_index_refs.py")
_spec = importlib.util.spec_from_file_location("check_agent_index_refs", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_agent_index_refs"] = checker
_spec.loader.exec_module(checker)


class ReferenceTests(unittest.TestCase):
    def test_extracts_relative_backticked_paths(self) -> None:
        text = "lee `AGENTS.md`, `docs/work-packages/BOARD.md` y `docs/status/*`"
        self.assertEqual(
            checker.referenced_paths(text), {"AGENTS.md", "docs/work-packages/BOARD.md"}
        )

    def test_resolves_bare_plan_names(self) -> None:
        text = "- `2026-11-24-wpt-harness-plan.md` — gate"
        self.assertEqual(
            checker.referenced_paths(text),
            {"docs/superpowers/plans/2026-11-24-wpt-harness-plan.md"},
        )

    def test_ignores_non_document_tokens(self) -> None:
        self.assertEqual(checker.referenced_paths("usa `WP-AUDIT` y `impact × frequency`"), set())


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs").mkdir()
        (self.root / "AGENTS.md").write_text("", encoding="utf-8")
        (self.root / "docs/AGENT-INDEX.md").write_text("lee `AGENTS.md`\n", encoding="utf-8")

    def test_valid_references_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_index_is_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_reference_is_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").write_text("lee `NO-SUCH.md`\n", encoding="utf-8")
        self.assertEqual(
            checker.validate(self.root),
            ["NO-SUCH.md: referenced in AGENT-INDEX.md but missing"],
        )

    def test_no_references_is_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").write_text("nada aquí\n", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["no document paths found in AGENT-INDEX.md"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_index(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("agent index references OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("AGENT INDEX:", result.stderr)


if __name__ == "__main__":
    unittest.main()
