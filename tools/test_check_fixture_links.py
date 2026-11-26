#!/usr/bin/env python3
"""Unit tests for tools/check_fixture_links.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_fixture_links.py")
_spec = importlib.util.spec_from_file_location("check_fixture_links", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_fixture_links"] = checker
_spec.loader.exec_module(checker)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        fixtures = self.root / "tests/fixtures"
        fixtures.mkdir(parents=True)
        (fixtures / "real.txt").write_text("x", encoding="utf-8")
        (fixtures / "link.txt").symlink_to("real.txt")

    def test_valid_symlink_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_fixtures_dir_is_reported(self) -> None:
        import shutil

        shutil.rmtree(self.root / "tests/fixtures")
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_broken_symlink_is_reported(self) -> None:
        (self.root / "tests/fixtures/link.txt").unlink()
        (self.root / "tests/fixtures/link.txt").symlink_to("absent.txt")
        self.assertTrue(any("broken symlink" in p for p in checker.validate(self.root)))

    def test_escaping_symlink_is_reported(self) -> None:
        outside = self.root.parent / "outside-fixture-target.txt"
        outside.write_text("x", encoding="utf-8")
        self.addCleanup(outside.unlink)
        (self.root / "tests/fixtures/link.txt").unlink()
        (self.root / "tests/fixtures/link.txt").symlink_to(outside)
        self.assertTrue(any("escapes the repository" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_fixtures(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fixture links OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("FIXTURE LINK:", result.stderr)


if __name__ == "__main__":
    unittest.main()
