#!/usr/bin/env python3
"""Unit tests for tools/check_coverage_json.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_coverage_json.py")
_spec = importlib.util.spec_from_file_location("check_coverage_json", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_coverage_json"] = checker
_spec.loader.exec_module(checker)

MANIFEST = """[[capability]]
id = "A"
state = "done"
area = "F1"
[[capability]]
id = "B"
state = "done"
area = "F2"
"""


class PercentTests(unittest.TestCase):
    def test_complete_is_100(self) -> None:
        self.assertEqual(checker.expected_percent(2, 2), 100)

    def test_partial_is_capped_at_99(self) -> None:
        self.assertEqual(checker.expected_percent(1, 2), 50)
        self.assertEqual(checker.expected_percent(99, 100), 99)

    def test_empty_known_is_zero(self) -> None:
        self.assertEqual(checker.expected_percent(0, 0), 0)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "coverage.toml").write_text(MANIFEST, encoding="utf-8")
        self._write({"schema": 1, "overall": {"implemented": 2, "partial": 0, "known": 2, "percent": 100},
                     "areas": [{"id": "F1", "implemented": 1, "known": 1},
                               {"id": "F2", "implemented": 1, "known": 1}]})

    def _write(self, data: dict) -> None:
        (self.root / "docs/status/coverage.json").write_text(json.dumps(data), encoding="utf-8")

    def test_valid_report_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/coverage.json").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_wrong_overall_value_is_reported(self) -> None:
        data = json.loads((self.root / "docs/status/coverage.json").read_text())
        data["overall"]["implemented"] = 1
        self._write(data)
        self.assertTrue(any("overall.implemented" in p for p in checker.validate(self.root)))

    def test_area_sum_mismatch_is_reported(self) -> None:
        data = json.loads((self.root / "docs/status/coverage.json").read_text())
        data["areas"][0]["implemented"] = 0
        self._write(data)
        self.assertTrue(any("areas sum" in p for p in checker.validate(self.root)))

    def test_non_integer_counters_are_reported(self) -> None:
        data = json.loads((self.root / "docs/status/coverage.json").read_text())
        data["areas"][0]["implemented"] = "1"
        data["areas"][1]["known"] = "1"
        self._write(data)
        problems = checker.validate(self.root)
        self.assertTrue(
            any("areas" in p and ("implemented" in p or "known" in p) for p in problems)
        )

    def test_empty_manifest_is_reported(self) -> None:
        (self.root / "docs/status/coverage.toml").write_text("", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["coverage.toml has no capabilities"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_report(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("coverage JSON consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("COVERAGE JSON:", result.stderr)


if __name__ == "__main__":
    unittest.main()
