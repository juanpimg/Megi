#!/usr/bin/env python3
"""Unit tests for tools/check_coverage_html_areas.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_coverage_html_areas.py")
_spec = importlib.util.spec_from_file_location("check_coverage_html_areas", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_coverage_html_areas"] = checker
_spec.loader.exec_module(checker)

JSON = {"areas": [{"id": "F1", "implemented": 2, "known": 3}]}
TOML = """[[capability]]
id = "A"
state = "done"
[[capability]]
id = "B"
state = "pending"
"""
HTML = (
    "<table><tr><td><b>F1</b> HTML</td><td>2</td><td>3</td><td>66%</td></tr></table>"
    "<h2>Aún faltan</h2><table><tr><td>B</td><td>pending</td></tr></table>"
)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "coverage.json").write_text(json.dumps(JSON), encoding="utf-8")
        (status / "coverage.toml").write_text(TOML, encoding="utf-8")
        (status / "coverage.html").write_text(HTML, encoding="utf-8")

    def test_valid_html_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/coverage.html").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_wrong_area_counts_are_reported(self) -> None:
        (self.root / "docs/status/coverage.html").write_text(
            HTML.replace("<td>2</td><td>3</td>", "<td>1</td><td>3</td>"), encoding="utf-8"
        )
        self.assertTrue(any("area F1 shows (1, 3)" in p for p in checker.validate(self.root)))

    def test_unfinished_capability_must_be_listed(self) -> None:
        (self.root / "docs/status/coverage.html").write_text(
            "<table><tr><td><b>F1</b></td><td>2</td><td>3</td></tr></table>", encoding="utf-8"
        )
        self.assertTrue(any("unfinished capability B" in p for p in checker.validate(self.root)))

    def test_no_area_rows_is_reported(self) -> None:
        (self.root / "docs/status/coverage.html").write_text("<p>sin tabla</p>", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["coverage.html: no per-area rows found"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_html(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("coverage HTML areas consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("COVERAGE HTML:", result.stderr)


if __name__ == "__main__":
    unittest.main()
