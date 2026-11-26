#!/usr/bin/env python3
"""Unit tests for tools/check_changelog_version.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_changelog_version.py")
_spec = importlib.util.spec_from_file_location("check_changelog_version", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_changelog_version"] = checker
_spec.loader.exec_module(checker)

CARGO = '[workspace.package]\nversion = "0.1.0"\n'
CHANGELOG = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-11-12\n"


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "Cargo.toml").write_text(CARGO, encoding="utf-8")
        (self.root / "CHANGELOG.md").write_text(CHANGELOG, encoding="utf-8")

    def test_valid_changelog_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "Cargo.toml").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_version_in_cargo_is_reported(self) -> None:
        (self.root / "Cargo.toml").write_text("[workspace.package]\n", encoding="utf-8")
        self.assertTrue(any("Cargo.toml: missing" in p for p in checker.validate(self.root)))

    def test_cargo_version_must_be_documented(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            CHANGELOG.replace("0.1.0", "0.2.0"), encoding="utf-8"
        )
        self.assertTrue(any("not documented" in p for p in checker.validate(self.root)))

    def test_duplicate_versions_are_reported(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            CHANGELOG + "## [0.1.0] - 2026-11-09\n", encoding="utf-8"
        )
        self.assertTrue(any("duplicate version" in p for p in checker.validate(self.root)))

    def test_missing_unreleased_section_is_reported(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            CHANGELOG.replace("## [Unreleased]\n", ""), encoding="utf-8"
        )
        self.assertTrue(any("missing [Unreleased]" in p for p in checker.validate(self.root)))

    def test_missing_date_in_release_is_reported(self) -> None:
        (self.root / "CHANGELOG.md").write_text(
            CHANGELOG + "## [0.0.9]\n", encoding="utf-8"
        )
        self.assertTrue(any("release 0.0.9 has no ISO date" in p for p in checker.validate(self.root)))

    def test_no_versions_is_reported(self) -> None:
        (self.root / "CHANGELOG.md").write_text("# Changelog\n", encoding="utf-8")
        self.assertTrue(any("no version headings" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_changelog(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("changelog version consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("CHANGELOG:", result.stderr)


if __name__ == "__main__":
    unittest.main()
