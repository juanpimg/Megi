#!/usr/bin/env python3
"""Tests for the partial benchmark-baseline addendum publisher."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).with_name("update_bench_baseline.py")
BASE_DOC = {
    "schema": "megi-bench-baseline-v1",
    "round": 36,
    "commit": "a080249",
    "rows": [{"name": "old-row", "group": "css", "median_ms": 1.5}],
}


def bench_doc(median: float, commit: str = "abc1234") -> dict:
    return {
        "schema": "megi-bench-v1",
        "commit": commit,
        "worktree_dirty": False,
        "rustc": "rustc 1.99.1",
        "cpu": "test CPU",
        "cores": 4,
        "iterations": 5,
        "warmup": 1,
        "results": [
            {
                "name": "js/hot-engine",
                "group": "js",
                "detail": "synthetic",
                "median_ms": median,
                "samples_ms": [median - 0.1, median, median + 0.1],
            }
        ],
    }


class UpdateBenchBaselineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.baseline = self.root / "baseline.json"
        self.baseline.write_text(json.dumps(BASE_DOC), encoding="utf-8")
        self.passes = []
        for index, median in enumerate((10.0, 12.0, 11.0), start=1):
            path = self.root / f"run{index}.json"
            path.write_text(json.dumps(bench_doc(median)), encoding="utf-8")
            self.passes.append(path)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def command(self, mode: str) -> list[str]:
        return [
            sys.executable,
            str(TOOL),
            "--baseline",
            str(self.baseline),
            "--id",
            "js-test",
            "--passes",
            *(str(path) for path in self.passes),
            "--date",
            "2026-11-25",
            "--method",
            "median of medians",
            "--load-note",
            "test load",
            "--artifacts",
            "run1..3.json",
            mode,
        ]

    def test_write_preserves_full_suite_and_publishes_median(self) -> None:
        result = subprocess.run(self.command("--write"), capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(self.baseline.read_text(encoding="utf-8"))
        self.assertEqual(payload["rows"], BASE_DOC["rows"])
        self.assertEqual(payload["round"], 36)
        addendum = payload["addenda"]["js-test"]
        self.assertEqual(addendum["passes"], 3)
        self.assertEqual(addendum["commit"], "abc1234")
        row = addendum["rows"][0]
        self.assertEqual(row["median_ms"], 11.0)
        self.assertEqual(row["median_range_ms"], [10.0, 12.0])
        self.assertEqual(row["name"], "js/hot-engine")

    def test_check_detects_stale_addendum(self) -> None:
        self.assertEqual(subprocess.run(self.command("--write"), capture_output=True).returncode, 0)
        self.assertEqual(subprocess.run(self.command("--check"), capture_output=True).returncode, 0)
        payload = json.loads(self.baseline.read_text(encoding="utf-8"))
        payload["addenda"]["js-test"]["rows"][0]["median_ms"] = 99.0
        self.baseline.write_text(json.dumps(payload), encoding="utf-8")
        result = subprocess.run(self.command("--check"), capture_output=True, text=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("stale or inconsistent", result.stdout)

    def test_mixed_commit_fails_before_writing(self) -> None:
        self.passes[1].write_text(json.dumps(bench_doc(12.0, "other")), encoding="utf-8")
        result = subprocess.run(self.command("--write"), capture_output=True, text=True)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertNotIn("addenda", json.loads(self.baseline.read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
