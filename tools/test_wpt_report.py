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
