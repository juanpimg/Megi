#!/usr/bin/env python3
"""Unit tests for tools/check_milestone_index.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_milestone_index.py")
_spec = importlib.util.spec_from_file_location("check_milestone_index", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_milestone_index"] = checker
_spec.loader.exec_module(checker)

INDEX = "| Hito | Nombre |\n|---|---|\n| M0 | a |\n| M1 | b |\n"
MANIFEST = '[[phase]]\nid = "M0"\n[[phase]]\nid = "M1"\n'


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs/status").mkdir(parents=True)
        (self.root / "docs/AGENT-INDEX.md").write_text(INDEX, encoding="utf-8")
        (self.root / "docs/status/progress.toml").write_text(MANIFEST, encoding="utf-8")

    def test_valid_milestones_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_milestone_only_in_index_is_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").write_text(INDEX + "| M2 | c |\n", encoding="utf-8")
        self.assertIn("M2: in AGENT-INDEX.md but not in progress.toml", checker.validate(self.root))

    def test_milestone_only_in_manifest_is_reported(self) -> None:
        (self.root / "docs/status/progress.toml").write_text(
            MANIFEST + '[[phase]]\nid = "M2"\n', encoding="utf-8"
        )
        self.assertIn("M2: in progress.toml but not in AGENT-INDEX.md", checker.validate(self.root))

    def test_duplicate_rows_are_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").write_text(INDEX + "| M0 | dup |\n", encoding="utf-8")
        self.assertTrue(any("duplicate milestone" in p for p in checker.validate(self.root)))

    def test_empty_index_is_reported(self) -> None:
        (self.root / "docs/AGENT-INDEX.md").write_text("sin tabla\n", encoding="utf-8")
        self.assertTrue(any("no milestones found" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_index(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("milestone index consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("MILESTONE INDEX:", result.stderr)


if __name__ == "__main__":
    unittest.main()
