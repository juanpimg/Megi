#!/usr/bin/env python3
"""Unit tests for tools/check_progress_consistency.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_progress_consistency.py")
_spec = importlib.util.spec_from_file_location("check_progress_consistency", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_progress_consistency"] = checker
_spec.loader.exec_module(checker)

SNAPSHOT = (
    "**Snapshot** — commit `65a8e084` (2026-11-26) · hitos cerrados **6/13** · "
    "criterios verificados **47/64** (**73%**) · **3,654** tests · conformance "
    "**PASS** · **18** crates · **270,520** lines of Rust"
)
PROGRESS = {
    "title": "Megi",
    "subtitle": "M0-M12",
    "last_updated_by": "tester",
    "stats": {
        "tests": 3654,
        "tests_commit": "65a8e084",
        "tests_date": "2026-11-26",
        "crates": 18,
        "rust_lines": 270520,
        "unsafe_forbidden_crates": 18,
    },
    "gate": {
        "status": "PASS",
        "verified_commit": "65a8e084",
        "tokenizer": "7050 / 7050",
        "tree": "3904 / 3904",
        "serializer": "230 / 230",
        "css_syntax": "100 / 100",
    },
    "phase": [{"id": "M0", "items": ["x"], "pending": []}],
}
COVERAGE = {
    "last_updated_by": "docs (test)",
    "capability": [
        {"id": "A", "name": "a", "state": "done", "evidence": "gate:x"},
        {"id": "B", "name": "b", "state": "pending", "evidence": "gap"},
    ],
}
HTML = (
    "<p>Actualizado por: docs (test)</p>\n"
    "<p><span class='big'>50%</span> <b>1/2</b> capacidades implementadas</p>"
)


class ProgressTests(unittest.TestCase):
    def test_matching_report_passes(self) -> None:
        self.assertEqual(checker.validate_progress(PROGRESS, SNAPSHOT), [])

    def test_missing_keys_are_reported(self) -> None:
        broken = {"stats": {}, "gate": {}}
        problems = checker.validate_progress(broken, SNAPSHOT)
        self.assertIn("progress.toml: missing title", problems)
        self.assertIn("progress.toml: [stats] missing tests", problems)
        self.assertIn("progress.toml: [gate] missing status", problems)
        self.assertIn("progress.toml: no [[phase]] entries", problems)

    def test_stale_test_count_is_reported(self) -> None:
        broken = dict(PROGRESS)
        broken["stats"] = dict(PROGRESS["stats"], tests=3000)
        problems = checker.validate_progress(broken, SNAPSHOT)
        self.assertIn("PROGRESS.md: tests=3654 but manifest says 3000", problems)

    def test_broken_snapshot_line_is_reported(self) -> None:
        problems = checker.validate_progress(PROGRESS, "no snapshot here")
        self.assertEqual(problems, ["PROGRESS.md: snapshot line is missing or malformed"])


class CoverageTests(unittest.TestCase):
    def test_matching_report_passes(self) -> None:
        self.assertEqual(checker.validate_coverage(COVERAGE, HTML), [])

    def test_wrong_headline_is_reported(self) -> None:
        problems = checker.validate_coverage(COVERAGE, HTML.replace("<b>1/2</b>", "<b>2/2</b>"))
        self.assertTrue(any("coverage.html: shows 2/2" in p for p in problems))

    def test_stale_update_stamp_is_reported(self) -> None:
        problems = checker.validate_coverage(COVERAGE, HTML.replace("docs (test)", "docs (old)"))
        self.assertTrue(any("update stamp differs" in p for p in problems))

    def test_incomplete_capability_is_reported(self) -> None:
        broken = {"last_updated_by": "x", "capability": [{"id": "A", "name": "", "state": "done"}]}
        problems = checker.validate_coverage(broken, HTML)
        self.assertTrue(any("capability[0] missing name" in p for p in problems))
        self.assertTrue(any("capability[0] missing evidence" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_repository(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("progress/coverage consistency OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("STATUS CONSISTENCY:", result.stderr)


if __name__ == "__main__":
    unittest.main()
