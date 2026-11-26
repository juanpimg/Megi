#!/usr/bin/env python3
"""Unit tests for tools/check_real_site_snapshots.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_real_site_snapshots.py")
_spec = importlib.util.spec_from_file_location("check_real_site_snapshots", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_real_site_snapshots"] = checker
_spec.loader.exec_module(checker)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.fixtures = Path(self.tmp.name)
        (self.fixtures / "example.html").write_text("<p>ok</p>", encoding="utf-8")

    def test_existing_snapshot_passes(self) -> None:
        self.assertEqual(
            checker.validate({"site": [{"id": "example", "snapshot": "example.html"}]}, self.fixtures),
            [],
        )

    def test_default_snapshot_name_uses_site_id(self) -> None:
        self.assertEqual(
            checker.validate({"site": [{"id": "example"}]}, self.fixtures), []
        )

    def test_missing_snapshot_is_reported(self) -> None:
        problems = checker.validate(
            {"site": [{"id": "rust", "snapshot": "rust.html"}]}, self.fixtures
        )
        self.assertEqual(
            problems,
            [f"rust: missing snapshot {(self.fixtures / 'rust.html').as_posix()}"],
        )

    def test_empty_snapshot_is_reported(self) -> None:
        (self.fixtures / "empty.html").write_text("", encoding="utf-8")
        problems = checker.validate(
            {"site": [{"id": "x", "snapshot": "empty.html"}]}, self.fixtures
        )
        self.assertEqual(
            problems,
            [f"x: empty snapshot {(self.fixtures / 'empty.html').as_posix()}"],
        )

    def test_empty_snapshot_name_is_reported(self) -> None:
        problems = checker.validate({"site": [{"id": "x", "snapshot": ""}]}, self.fixtures)
        self.assertEqual(problems, ["x: empty snapshot name"])

    def test_manifest_without_sites_is_reported(self) -> None:
        self.assertEqual(checker.validate({}, self.fixtures), ["manifest has no [[site]] entries"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_manifest(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("snapshots OK (4 sites)", result.stdout)

    def test_cli_reports_a_missing_fixture_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--fixtures", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("SNAPSHOT:", result.stderr)


if __name__ == "__main__":
    unittest.main()
