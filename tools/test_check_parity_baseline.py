#!/usr/bin/env python3
"""Unit tests for tools/check_parity_baseline.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_parity_baseline.py")
_spec = importlib.util.spec_from_file_location("check_parity_baseline", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_parity_baseline"] = checker
_spec.loader.exec_module(checker)


def metrics(pct: float, mean: float) -> dict:
    return {"size": [1024, 768], "changed_pct": pct, "mean_abs_diff": mean}


class RegressionsTests(unittest.TestCase):
    def test_equal_report_has_no_regressions(self) -> None:
        baseline = {"example": metrics(0.92, 1.246)}
        self.assertEqual(checker.regressions(baseline, baseline, 0.5, 1.0), [])

    def test_small_changes_within_tolerance_are_allowed(self) -> None:
        baseline = {"example": metrics(0.92, 1.246)}
        current = {"example": metrics(1.2, 2.0)}
        self.assertEqual(checker.regressions(baseline, current, 0.5, 1.0), [])

    def test_growth_beyond_tolerance_is_reported(self) -> None:
        baseline = {"example": metrics(0.92, 1.246)}
        current = {"example": metrics(2.0, 3.0)}
        problems = checker.regressions(baseline, current, 0.5, 1.0)
        self.assertEqual(len(problems), 2)
        self.assertTrue(all("example" in problem for problem in problems))

    def test_missing_site_is_reported(self) -> None:
        problems = checker.regressions({"rust": metrics(1.0, 1.0)}, {}, 0.5, 1.0)
        self.assertEqual(problems, ["rust: missing from the new report"])

    def test_viewport_change_is_reported(self) -> None:
        baseline = {"example": metrics(0.92, 1.246)}
        current = {"example": metrics(0.92, 1.246)}
        current["example"]["size"] = [800, 600]
        problems = checker.regressions(baseline, current, 0.5, 1.0)
        self.assertEqual(len(problems), 1)
        self.assertIn("viewport", problems[0])

    def test_new_site_is_informational_not_a_regression(self) -> None:
        baseline = {"example": metrics(0.92, 1.246)}
        current = dict(baseline)
        current["new"] = metrics(9.0, 9.0)
        self.assertEqual(checker.regressions(baseline, current, 0.5, 1.0), [])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_versioned_baseline_and_a_matching_report(self) -> None:
        root = Path(__file__).resolve().parent.parent
        baseline = root / "tests/fixtures/visual-parity/baseline.json"
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "report.json"
            report.write_text(baseline.read_text(encoding="utf-8"), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), str(report), "--baseline", str(baseline)],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("parity baseline OK", result.stdout)

    def test_cli_rejects_negative_tolerance(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "report.json"
            report.write_text("{}", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), str(report), "--tol-pct", "-1"],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("tolerances must be >= 0", result.stderr)


if __name__ == "__main__":
    unittest.main()
