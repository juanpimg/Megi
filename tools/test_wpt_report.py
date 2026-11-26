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

from tools import wpt_report

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


class WptReportScopeTests(unittest.TestCase):
    def test_repository_report_publishes_the_excluded_denominator(self) -> None:
        """#2432: 596/596 must be labelled as partial WPT coverage, not full conformance."""
        path = Path(__file__).resolve().parent.parent / wpt_report.STATUS_DIR / "wpt.json"
        report = json.loads(path.read_text(encoding="utf-8"))
        scope = report["run_info"]["scope"]
        run_info = report["run_info"]
        self.assertIn("adopted", scope)
        self.assertIn("excluded", scope)
        self.assertIn("NOT full WPT conformance", scope)
        self.assertGreater(run_info["excluded_files"], 0)
        self.assertEqual(
            run_info["excluded_files"], sum(run_info["excluded_by_kind"].values())
        )
        self.assertTrue(run_info["debt_dirs"])


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
