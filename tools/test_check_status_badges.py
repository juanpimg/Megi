#!/usr/bin/env python3
"""Unit tests for tools/check_status_badges.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_status_badges.py")
_spec = importlib.util.spec_from_file_location("check_status_badges", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_status_badges"] = checker
_spec.loader.exec_module(checker)

BADGE = '<svg aria-label="conformance: PASS · 4 suites"><title>x</title></svg>'


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "README.md").write_text(
            "![Tests](docs/status/badge-tests.svg)\n", encoding="utf-8"
        )
        (self.root / "PROGRESS.md").write_text(
            "![Conformance](docs/status/badge-conformance.svg)\n", encoding="utf-8"
        )
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "badge-tests.svg").write_text(BADGE, encoding="utf-8")
        (status / "badge-conformance.svg").write_text(BADGE, encoding="utf-8")
        (status / "progress.toml").write_text('[gate]\nstatus = "PASS"\n', encoding="utf-8")

    def test_valid_badges_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_badge_is_reported(self) -> None:
        (self.root / "docs/status/badge-tests.svg").unlink()
        self.assertIn("docs/status/badge-tests.svg: referenced badge is missing", checker.validate(self.root))

    def test_conformance_mismatch_is_reported(self) -> None:
        (self.root / "docs/status/progress.toml").write_text('[gate]\nstatus = "FAIL"\n', encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("disagrees with gate" in p for p in problems))

    def test_missing_aria_label_is_reported(self) -> None:
        (self.root / "docs/status/badge-conformance.svg").write_text("<svg></svg>", encoding="utf-8")
        self.assertIn("badge-conformance.svg: missing aria-label", checker.validate(self.root))

    def test_missing_docs_are_reported(self) -> None:
        (self.root / "README.md").unlink()
        (self.root / "PROGRESS.md").unlink()
        problems = checker.validate(self.root)
        self.assertEqual(len(problems), 3)  # two missing docs + no references
        self.assertTrue(any("no status badge references" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_badges(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("status badges consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("STATUS BADGE:", result.stderr)


if __name__ == "__main__":
    unittest.main()
