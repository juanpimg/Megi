#!/usr/bin/env python3
"""Tests for the honest Test262-subset report, pin verification, gate and WP feed."""

from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import test262_gate, test262_report, wp_generate

SCOPE = "ECMAScript language expressions/smoke subset, NOT full Test262 conformance"
CASE = "language/expressions/arithmetic/addition-numbers.js"


def make_report(status: str = "PASS") -> dict:
    test = f"tests/test262/{CASE}"
    key = hashlib.sha256(f"{test}\0{CASE}".encode("utf-8")).hexdigest()
    counts = {field: 0 for field in ("pass", "fail", "timeout", "notrun", "error")}
    counts["pass" if status == "PASS" else "fail"] = 1
    return {
        "schema": "megi-test262-report/1",
        "epoch": {"manifest_sha256": "a" * 64, "scope": SCOPE},
        "run_info": {"harness": "stub", "scope": SCOPE, "source": "stub",
                     "adopted_cases": 1, "dirs": ["language/expressions/arithmetic"]},
        "counts": counts,
        "total": 1,
        "results": [{"key": key, "test": test, "subtest": CASE, "status": status,
                     "sha256": "b" * 64}],
        "allowlist": 0,
        "headline": "Test262 aaaaaaa — 1/1 cases passed (dirs: 1)",
    }


class Test262ReportTests(unittest.TestCase):
    def check(self, root: Path, report: dict) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(test262_report, "build_report", return_value=copy.deepcopy(report)):
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                rc = test262_report.main(["--check", "--root", str(root)])
        return rc, stdout.getvalue(), stderr.getvalue()

    def test_check_reports_stale_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc, _out, err = self.check(Path(tmp), make_report())
            self.assertEqual(rc, 1)
            self.assertIn("stale: test262.json, test262.md", err)
            self.assertNotIn("Traceback", err)

    def test_check_passes_on_fresh_artifacts(self) -> None:
        report = make_report()
        with tempfile.TemporaryDirectory() as tmp:
            status = Path(tmp) / test262_report.STATUS_DIR
            status.mkdir(parents=True)
            (status / "test262.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            (status / "test262.md").write_text(test262_report.render_markdown(report), encoding="utf-8")
            rc, out, err = self.check(Path(tmp), report)
            self.assertEqual(rc, 0, err)
            self.assertIn("up to date", out)

    def test_real_failure_is_published_instead_of_raising(self) -> None:
        # #2440: build_report must publish the honest distribution; the gate, not the
        # report writer, decides whether a FAIL is acceptable.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            case_path = root / "tests/test262" / CASE
            case_path.parent.mkdir(parents=True)
            source = b"assert.sameValue(1, 1);"
            case_path.write_bytes(source)
            manifest = {
                "schema": test262_report.MANIFEST_SCHEMA,
                "scope": SCOPE,
                "source": "stub",
                "cases": [{
                    "path": CASE,
                    "sha256": hashlib.sha256(source).hexdigest(),
                    "bytes": len(source),
                    "dir": "language/expressions/arithmetic",
                }],
            }
            (root / test262_report.MANIFEST_REL).write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            output = (
                "--- Test262 language/expressions/arithmetic Suites "
                "(tests/test262/language/expressions/arithmetic) ---\n"
                f"  [FAIL] {CASE}\n"
                "language/expressions/arithmetic Total: 0 / 1 tests passed\n"
                "Test262 subset: 0 / 1 cases passed\n"
            )
            report = test262_report.build_report(root, output=output)
        self.assertEqual(report["total"], 1)
        self.assertEqual(report["counts"]["pass"], 0)
        self.assertEqual(report["counts"]["fail"], 1)
        self.assertIn("FAIL", report["headline"])
        self.assertEqual(report["results"][0]["status"], "FAIL")

    def test_pin_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "tests/test262"
            base.mkdir(parents=True)
            (base / CASE).parent.mkdir(parents=True, exist_ok=True)
            (base / CASE).write_bytes(b"assert.sameValue(1, 1);")
            manifest = {
                "schema": "megi-test262-manifest/1",
                "scope": SCOPE,
                "source": "stub",
                "cases": [{"path": CASE, "sha256": "0" * 64, "bytes": 23, "dir": "language/expressions/arithmetic"}],
            }
            (base / "MANIFEST.json").write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaises(ValueError) as caught:
                test262_report.load_manifest(root)
            self.assertIn("pin mismatch", str(caught.exception))


class Test262GateTests(unittest.TestCase):
    def test_clean_report_has_clean_allowlist(self) -> None:
        baseline = {"epoch": {"manifest_sha256": "a" * 64, "scope": SCOPE},
                    "passed": 1, "total": 1, "allowlist": []}
        result = {"test": f"tests/test262/{CASE}", "subtest": CASE, "status": "PASS"}
        loaded = {"epoch": baseline["epoch"], "counts": {"pass": 1, "fail": 0, "timeout": 0,
                  "notrun": 0, "error": 0}, "total": 1, "results": [result]}
        self.assertIsNone(test262_gate.check_gate(loaded, baseline, []))

    def test_unallowlisted_failure_is_rejected(self) -> None:
        baseline = {"epoch": {"manifest_sha256": "a" * 64, "scope": SCOPE},
                    "passed": 1, "total": 1, "allowlist": []}
        result = {"test": f"tests/test262/{CASE}", "subtest": CASE, "status": "FAIL"}
        loaded = {"epoch": baseline["epoch"], "counts": {"pass": 0, "fail": 1, "timeout": 0,
                  "notrun": 0, "error": 0}, "total": 1, "results": [result]}
        failure = test262_gate.check_gate(loaded, baseline, [])
        self.assertIsNotNone(failure)
        self.assertIn("failure not allowlisted", failure)


class Js262GeneratorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write(self, report: dict) -> None:
        path = self.root / "docs/status/test262.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(report), encoding="utf-8")

    def test_passing_report_produces_zero_wps(self) -> None:
        self._write(make_report("PASS"))
        self.assertEqual(wp_generate.generate("js262", root=self.root), [])

    def test_failing_case_produces_one_wp(self) -> None:
        self._write(make_report("FAIL"))
        wps = wp_generate.generate("js262", root=self.root)
        self.assertEqual(len(wps), 1)
        self.assertEqual(wps[0].metric, "js262_passed")
        self.assertIn("test262_gate.py", wps[0].verify_cmd)


if __name__ == "__main__":
    unittest.main()
