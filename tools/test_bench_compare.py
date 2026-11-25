#!/usr/bin/env python3
"""Unit tests for tools/bench_compare.py (benchmark regression gate)."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
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

    def _run(self, *extra: str) -> tuple[int, str]:
        argv = ["bench_compare.py", str(self._actual), "--baseline", str(self._baseline), *extra]
        old = sys.argv
        sys.argv = argv
        stderr = StringIO()
        try:
            with redirect_stdout(StringIO()), redirect_stderr(stderr):
                try:
                    rc = bench_compare.main()
                except SystemExit as exc:
                    rc = int(exc.code)
            return rc, stderr.getvalue()
        finally:
            sys.argv = old

    def test_missing_baseline_row_fails_the_gate(self) -> None:
        # #1836: a fresh run that silently drops the slow row must not report
        # "no regressions" and exit 0 -- that defeats the only perf gate.
        rc, _ = self._run()
        self.assertEqual(rc, 1)

    def test_allow_missing_restores_old_behavior(self) -> None:
        rc, _ = self._run("--allow-missing")
        self.assertEqual(rc, 0)

    def test_non_dict_actual_document_exits_with_code_2(self) -> None:
        self._actual.write_text("[]", encoding="utf-8")
        rc, err = self._run()
        self.assertEqual(rc, 2)
        self.assertIn("error:", err)

    def test_non_dict_baseline_document_exits_with_code_2(self) -> None:
        self._baseline.write_text("[]", encoding="utf-8")
        rc, err = self._run()
        self.assertEqual(rc, 2)
        self.assertIn("error:", err)


class JsonStdoutTests(unittest.TestCase):
    """`--json` must keep stdout machine-readable (#2290 triage).

    A regression (or a missing baseline row) appends a human diagnostic after
    the JSON document. That diagnostic belongs on stderr whenever --json is
    active, otherwise `json.loads(stdout)` fails with "Extra data".
    """

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self._dir = Path(self._tmp.name)
        self._baseline = self._dir / "baseline.json"
        self._baseline.write_text(
            json.dumps(baseline_doc({"row": 10.0})), encoding="utf-8"
        )

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, rows: dict[str, float], *extra: str) -> tuple[int, str, str]:
        actual = self._dir / "actual.json"
        actual.write_text(json.dumps(bench_doc(rows)), encoding="utf-8")
        argv = ["bench_compare.py", str(actual), "--baseline", str(self._baseline), *extra]
        old = sys.argv
        sys.argv = argv
        stdout, stderr = StringIO(), StringIO()
        try:
            with redirect_stdout(stdout), redirect_stderr(stderr):
                try:
                    rc = bench_compare.main()
                except SystemExit as exc:
                    rc = int(exc.code)
            return rc, stdout.getvalue(), stderr.getvalue()
        finally:
            sys.argv = old

    def test_json_regression_stdout_is_pure_json(self) -> None:
        rc, out, _ = self._run({"row": 20.0}, "--json")
        self.assertEqual(rc, 1)
        payload = json.loads(out)
        self.assertEqual(payload["regressions"], ["row"])

    def test_json_regression_diagnostic_is_on_stderr(self) -> None:
        rc, _, err = self._run({"row": 20.0}, "--json")
        self.assertEqual(rc, 1)
        self.assertIn("1 regression(s): row", err)

    def test_json_missing_stdout_is_pure_json(self) -> None:
        rc, out, _ = self._run({}, "--json")
        self.assertEqual(rc, 1)
        payload = json.loads(out)
        self.assertEqual(payload["missing"], ["row"])

    def test_json_missing_diagnostic_is_on_stderr(self) -> None:
        rc, _, err = self._run({}, "--json")
        self.assertEqual(rc, 1)
        self.assertIn("1 missing baseline row(s): row", err)

    def test_plain_regression_output_unchanged(self) -> None:
        rc, out, err = self._run({"row": 20.0})
        self.assertEqual(rc, 1)
        self.assertIn("1 regression(s): row", out)
        self.assertEqual(err, "")

    def test_plain_missing_output_unchanged(self) -> None:
        rc, out, err = self._run({})
        self.assertEqual(rc, 1)
        self.assertIn("1 missing baseline row(s): row", out)
        self.assertEqual(err, "")


if __name__ == "__main__":
    unittest.main()
