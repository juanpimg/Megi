#!/usr/bin/env python3
"""Unit tests for tools/check_progress_svg.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_progress_svg.py")
_spec = importlib.util.spec_from_file_location("check_progress_svg", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_progress_svg"] = checker
_spec.loader.exec_module(checker)

MD = (
    "**Snapshot** — commit `abc1234` (2026-11-26) · hitos cerrados **1/1** · "
    "criterios verificados **1/1** (**100%**) · **1,234** tests · conformance "
    "**PASS** · **18** crates · **270,000** lines of Rust"
)
SVG = (
    "<svg><text>commit abc1234 (2026-11-26) · 18 crates · 1,234 tests · "
    "conformance PASS · 270,000 lines of Rust</text></svg>"
)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs/status").mkdir(parents=True)
        (self.root / "PROGRESS.md").write_text(MD, encoding="utf-8")
        (self.root / "docs/status/progress.svg").write_text(SVG, encoding="utf-8")

    def test_valid_artifacts_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "PROGRESS.md").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_snapshot_is_reported(self) -> None:
        (self.root / "PROGRESS.md").write_text("sin snapshot\n", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["PROGRESS.md: snapshot line is missing"])

    def test_missing_svg_footer_is_reported(self) -> None:
        (self.root / "docs/status/progress.svg").write_text("<svg></svg>", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["progress.svg: footer snapshot is missing"])

    def test_stale_test_count_is_reported(self) -> None:
        (self.root / "docs/status/progress.svg").write_text(
            SVG.replace("1,234 tests", "1,235 tests"), encoding="utf-8"
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("tests=1,235 but PROGRESS.md says 1,234" in p for p in problems))

    def test_stale_commit_is_reported(self) -> None:
        (self.root / "docs/status/progress.svg").write_text(
            SVG.replace("abc1234", "def5678"), encoding="utf-8"
        )
        self.assertTrue(any("commit=def5678" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_artifacts(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("progress SVG consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("PROGRESS SVG:", result.stderr)


if __name__ == "__main__":
    unittest.main()
