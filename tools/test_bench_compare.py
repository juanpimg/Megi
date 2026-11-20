#!/usr/bin/env python3
"""Unit tests for tools/bench_compare.py (benchmark regression gate)."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("bench_compare.py")
_spec = importlib.util.spec_from_file_location("bench_compare", MODULE_PATH)
bench_compare = importlib.util.module_from_spec(_spec)
sys.modules["bench_compare"] = bench_compare
_spec.loader.exec_module(bench_compare)


def bench_doc(rows: dict[str, float]) -> dict:
    return {
        "schema": "megi-bench-v1",
        "results": [{"name": name, "median_ms": ms} for name, ms in rows.items()],
    }


def baseline_doc(rows: dict[str, float]) -> dict:
    return {"schema": "megi-bench-baseline-v1", "rows": [
        {"name": name, "median_ms": ms, "group": "x"} for name, ms in rows.items()
    ]}


class MissingRowsTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self._dir = Path(self._tmp.name)
        self._baseline = self._dir / "baseline.json"
        self._actual = self._dir / "actual.json"
        self._baseline.write_text(json.dumps(baseline_doc({"slow": 10.0, "fast": 2.0})), encoding="utf-8")
        # The fresh run dropped "slow" (the very row that would have regressed).
        self._actual.write_text(json.dumps(bench_doc({"fast": 2.0})), encoding="utf-8")

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, *extra: str) -> int:
        argv = ["bench_compare.py", str(self._actual), "--baseline", str(self._baseline), *extra]
        old = sys.argv
        sys.argv = argv
        try:
            with redirect_stdout(StringIO()):
                return bench_compare.main()
        finally:
            sys.argv = old

    def test_missing_baseline_row_fails_the_gate(self) -> None:
        # #1836: a fresh run that silently drops the slow row must not report
        # "no regressions" and exit 0 -- that defeats the only perf gate.
        self.assertEqual(self._run(), 1)

    def test_allow_missing_restores_old_behavior(self) -> None:
        self.assertEqual(self._run("--allow-missing"), 0)


if __name__ == "__main__":
    unittest.main()
