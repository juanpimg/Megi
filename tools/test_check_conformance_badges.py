#!/usr/bin/env python3
"""Unit tests for tools/check_conformance_badges.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_conformance_badges.py")
_spec = importlib.util.spec_from_file_location("check_conformance_badges", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_conformance_badges"] = checker
_spec.loader.exec_module(checker)

MANIFEST = "[wpt]\npassed = 5\ntotal = 6\n[test262]\npassed = 2\ntotal = 2\n"


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "progress.toml").write_text(MANIFEST, encoding="utf-8")
        (status / "badge-wpt.svg").write_text('<svg aria-label="wpt: 5/6">', encoding="utf-8")
        (status / "badge-test262.svg").write_text('<svg aria-label="test262: 2/2">', encoding="utf-8")

    def test_valid_badges_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_badge_is_reported(self) -> None:
        (self.root / "docs/status/badge-wpt.svg").unlink()
        self.assertTrue(any("badge-wpt.svg" in p for p in checker.validate(self.root)))

    def test_malformed_label_is_reported(self) -> None:
        (self.root / "docs/status/badge-wpt.svg").write_text("<svg></svg>", encoding="utf-8")
        self.assertTrue(any("malformed aria-label" in p for p in checker.validate(self.root)))

    def test_stale_counts_are_reported(self) -> None:
        (self.root / "docs/status/badge-test262.svg").write_text(
            '<svg aria-label="test262: 1/2">', encoding="utf-8"
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("shows 1/2, progress.toml says 2/2" in p for p in problems))

    def test_missing_manifest_is_reported(self) -> None:
        (self.root / "docs/status/progress.toml").unlink()
        self.assertEqual(checker.validate(self.root), ["missing docs/status/progress.toml"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_badges(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("conformance badges consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("CONFORMANCE BADGE:", result.stderr)


if __name__ == "__main__":
    unittest.main()
