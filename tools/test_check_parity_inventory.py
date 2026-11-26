#!/usr/bin/env python3
"""Unit tests for tools/check_parity_inventory.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_parity_inventory.py")
_spec = importlib.util.spec_from_file_location("check_parity_inventory", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_parity_inventory"] = checker
_spec.loader.exec_module(checker)


class DiscoverTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs").mkdir()
        (self.root / "tools").mkdir()
        (self.root / "tests/fixtures/visual-parity").mkdir(parents=True)
        (self.root / "docs/VISUAL-PARITY.md").write_text("x", encoding="utf-8")
        (self.root / "docs/DESLOP-AUDIT.md").write_text("x", encoding="utf-8")
        (self.root / "tools/check_parity_demo.py").write_text("x", encoding="utf-8")
        (self.root / "tools/unrelated.py").write_text("x", encoding="utf-8")
        (self.root / "tests/fixtures/visual-parity/baseline.json").write_text("{}", encoding="utf-8")

    def test_discovers_docs_fixtures_and_parity_tools(self) -> None:
        found = checker.discover(self.root)
        self.assertIn("docs/VISUAL-PARITY.md", found)
        self.assertIn("docs/DESLOP-AUDIT.md", found)
        self.assertIn("tools/check_parity_demo.py", found)
        self.assertIn("tests/fixtures/visual-parity/baseline.json", found)
        self.assertNotIn("tools/unrelated.py", found)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "tools").mkdir()
        (self.root / "tools/check_parity_demo.py").write_text("x", encoding="utf-8")

    def test_matching_inventory_has_no_problems(self) -> None:
        self.assertEqual(
            checker.validate(self.root, ("tools/check_parity_demo.py",)), []
        )

    def test_undeclared_artifact_is_reported(self) -> None:
        problems = checker.validate(self.root, ())
        self.assertEqual(
            problems,
            ["tools/check_parity_demo.py: parity artifact is not declared in DEFAULT_PATHS"],
        )

    def test_stale_declaration_is_reported(self) -> None:
        problems = checker.validate(
            self.root, ("tools/check_parity_demo.py", "tools/check_parity_missing.py")
        )
        self.assertEqual(
            problems,
            ["tools/check_parity_missing.py: declared in DEFAULT_PATHS but the file does not exist"],
        )


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_inventory(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("inventory OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("INVENTORY:", result.stderr)


if __name__ == "__main__":
    unittest.main()
