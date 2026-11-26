#!/usr/bin/env python3
"""Unit tests for tools/check_coverage_svg.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_coverage_svg.py")
_spec = importlib.util.spec_from_file_location("check_coverage_svg", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_coverage_svg"] = checker
_spec.loader.exec_module(checker)

REPORT = {
    "overall": {"implemented": 2, "known": 2, "percent": 100},
    "areas": [{"id": "F1", "implemented": 1, "known": 1},
              {"id": "F2", "implemented": 1, "known": 1}],
}
SVG = "<svg><text>100%</text><text>2/2</text><text>1/1</text></svg>"


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "coverage.json").write_text(json.dumps(REPORT), encoding="utf-8")
        (status / "coverage.svg").write_text(SVG, encoding="utf-8")

    def test_valid_svg_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/coverage.svg").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_headline_is_reported(self) -> None:
        (self.root / "docs/status/coverage.svg").write_text("<svg>100%</svg>", encoding="utf-8")
        self.assertTrue(any("missing overall headline 2/2" in p for p in checker.validate(self.root)))

    def test_missing_percent_is_reported(self) -> None:
        (self.root / "docs/status/coverage.svg").write_text("<svg>2/2</svg>", encoding="utf-8")
        self.assertTrue(any("missing percent 100%" in p for p in checker.validate(self.root)))

    def test_missing_area_count_is_reported(self) -> None:
        report = dict(REPORT)
        report["areas"] = [{"id": "F3", "implemented": 2, "known": 3}]
        (self.root / "docs/status/coverage.json").write_text(json.dumps(report), encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertIn("coverage.svg: missing area count 2/3 (F3)", problems)


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_svg(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("coverage SVG consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("COVERAGE SVG:", result.stderr)


if __name__ == "__main__":
    unittest.main()
