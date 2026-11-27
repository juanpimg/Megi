#!/usr/bin/env python3
"""Tests for the artifact comparison in `tools/wpt_report.py --check`."""

from __future__ import annotations

import contextlib
import copy
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import wpt_gate, wpt_report

REPORT = {
    "schema": "megi-wpt-report/1",
    "epoch": {"wpt_revision": "a" * 40, "manifest_sha256": "b" * 64},
    "run_info": {
        "harness": "stub",
        "scope": "deterministic fixture",
        "adopted_subtests": 2,
        "dirs": ["tests/wpt/css/css-syntax"],
    },
    "counts": {"pass": 2, "fail": 0, "timeout": 0, "notrun": 0, "error": 0},
    "total": 2,
    "results": [],
}


class LoadReportTests(unittest.TestCase):
    """#2448: claimed numbers must agree with the recorded results."""

    def _write(self, report: dict) -> Path:
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        path = Path(temp.name) / "wpt.json"
        path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        return path

    def test_inflated_counts_and_total_are_rejected(self) -> None:
        forged = copy.deepcopy(REPORT)
        forged["counts"]["pass"] = 99
        forged["total"] = 99
        with self.assertRaises(ValueError) as caught:
            wpt_report.load_report(self._write(forged))
        self.assertIn("counts do not match results", str(caught.exception))

    def test_forged_result_key_is_rejected(self) -> None:
        forged = copy.deepcopy(REPORT)
        forged["results"].append({
            "key": "0" * 64, "test": "x.html", "subtest": "x", "status": "PASS",
        })
        forged["counts"]["pass"] = 3
        forged["total"] = 3
        with self.assertRaises(ValueError) as caught:
            wpt_report.load_report(self._write(forged))
        self.assertIn("key does not match", str(caught.exception))

    def test_valid_counts_and_total_are_derived_from_results(self) -> None:
        report = copy.deepcopy(REPORT)
        result = {
            "key": wpt_gate.canonical_key("x.html", "x"),
            "test": "x.html", "subtest": "x", "status": "FAIL",
        }
        report["results"] = [result]
        report["counts"] = {"pass": 0, "fail": 1, "timeout": 0, "notrun": 0, "error": 0}
        report["total"] = 1
        loaded = wpt_report.load_report(self._write(report))
        self.assertEqual(loaded["counts"]["fail"], 1)
        self.assertEqual(loaded["counts"]["pass"], 0)
        self.assertEqual(loaded["total"], 1)


class WptReportCheckTests(unittest.TestCase):
    def check(self, root: Path) -> tuple[int, str, str]:
        """Run --check against a temp root without invoking cargo or the network."""
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(wpt_report, "build_report", return_value=copy.deepcopy(REPORT)):
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                rc = wpt_report.main(["--check", "--root", str(root)])
        return rc, stdout.getvalue(), stderr.getvalue()

    def test_missing_artifacts_are_stale_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc, _stdout, stderr = self.check(Path(tmp))
            self.assertEqual(rc, 1)
            self.assertIn("stale: wpt.json, wpt.md", stderr)
            self.assertNotIn("Traceback", stderr)

    def test_fresh_artifacts_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            status = Path(tmp) / wpt_report.STATUS_DIR
            status.mkdir(parents=True)
            (status / "wpt.json").write_text(
                json.dumps(REPORT, indent=2) + "\n", encoding="utf-8"
            )
            (status / "wpt.md").write_text(wpt_report.render_markdown(REPORT), encoding="utf-8")
            rc, stdout, stderr = self.check(Path(tmp))
            self.assertEqual(rc, 0, stderr)
            self.assertIn("up to date", stdout)


if __name__ == "__main__":
    unittest.main()
