#!/usr/bin/env python3
"""Tests for the WPT P/T monotonic gate (plan Task 4)."""

from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from tools import wpt_gate

ROOT = Path(__file__).resolve().parent.parent
TEST = "tests/wpt/css/css-syntax/anb-parsing.html"
NAME = "anb-parsing.html: odd"


def key(test: str = TEST, name: str = NAME) -> str:
    return hashlib.sha256(f"{test}\0{name}".encode("utf-8")).hexdigest()


def report(statuses: dict[str, str]) -> dict:
    results = [
        {"key": key(), "test": TEST, "subtest": NAME, "status": statuses.get(NAME, "PASS")},
        {
            "key": key(TEST, "anb-parsing.html: even"),
            "test": TEST,
            "subtest": "anb-parsing.html: even",
            "status": statuses.get("even", "PASS"),
        },
    ]
    counts = {"pass": 0, "fail": 0, "timeout": 0, "notrun": 0, "error": 0}
    for result in results:
        counts[wpt_gate.COUNT_FIELD[result["status"]]] += 1
    return {
        "epoch": {"wpt_revision": "ab2ef95", "manifest_sha256": "f" * 64},
        "counts": counts,
        "total": len(results),
        "results": results,
    }


def baseline(allow: list[dict] | None = None, passed: int = 2, total: int = 2) -> dict:
    return {
        "epoch": {"wpt_revision": "ab2ef95", "manifest_sha256": "f" * 64},
        "passed": passed,
        "total": total,
        "allowlist": allow or [],
    }


def entry(status: str = "FAIL") -> dict:
    return {
        "key": key(),
        "test": TEST,
        "subtest": NAME,
        "status": status,
        "reason": "",
        "issue": "",
        "spec": "",
    }


class WptGateTest(unittest.TestCase):
    def test_canonical_key_matches_sha256_test_nul_subtest(self) -> None:
        self.assertEqual(wpt_gate.canonical_key(TEST, NAME), key())

    def test_clean_report_passes(self) -> None:
        self.assertIsNone(wpt_gate.check_gate(report({}), baseline(), []))

    def test_unallowlisted_failure_fails(self) -> None:
        error = wpt_gate.check_gate(report({NAME: "FAIL"}), baseline(), [])
        self.assertIsNotNone(error)
        self.assertIn("not allowlisted", error)

    def test_pass_regression_is_rejected(self) -> None:
        error = wpt_gate.check_gate(
            report({NAME: "FAIL"}), baseline(allow=[entry("PASS")]), [entry("PASS")]
        )
        self.assertIsNotNone(error)
        self.assertIn("regressed", error)

    def test_fail_to_pass_improvement_is_accepted(self) -> None:
        current = report({NAME: "PASS"})
        self.assertIsNone(
            wpt_gate.check_gate(current, baseline(allow=[entry("FAIL")], passed=1), [entry("FAIL")])
        )

    def test_new_allowlist_key_is_rejected(self) -> None:
        current = report({NAME: "FAIL"})
        new_key = hashlib.sha256(b"new\0subtest").hexdigest()
        allowed = [entry(), dict(entry(), key=new_key, subtest="other")]
        recorded = [entry(), dict(entry(), key="b" * 64, subtest="other")]
        error = wpt_gate.check_gate(current, baseline(allow=recorded), allowed)
        self.assertIsNotNone(error)
        self.assertIn("new allowlist key", error)

    def test_growing_allowlist_is_rejected(self) -> None:
        current = report({NAME: "FAIL"})
        allowed = [entry(), dict(entry(), key="x" * 64, subtest="other")]
        error = wpt_gate.check_gate(current, baseline(allow=[entry()]), allowed)
        self.assertIsNotNone(error)
        self.assertIn("allowlist grew", error)

    def test_wildcard_is_rejected(self) -> None:
        allowed = [dict(entry(), subtest="anb-parsing.html: *")]
        error = wpt_gate.check_gate(report({NAME: "FAIL"}), baseline(allow=allowed), allowed)
        self.assertIsNotNone(error)
        self.assertIn("wildcard", error)

    def test_epoch_change_is_rejected(self) -> None:
        current = report({})
        current["epoch"]["manifest_sha256"] = "0" * 64
        error = wpt_gate.check_gate(current, baseline(), [])
        self.assertIsNotNone(error)
        self.assertIn("epoch changed", error)

    def test_shrinking_passed_or_total_is_rejected(self) -> None:
        current = report({})
        self.assertIn("passed", wpt_gate.check_gate(current, baseline(passed=3), []) or "")
        self.assertIn("total", wpt_gate.check_gate(current, baseline(total=3), []) or "")

    def test_report_without_string_subtest_is_rejected(self) -> None:
        """A forged key must not hide a missing/non-string subtest field."""
        current = report({})
        current["results"][0]["test"] = ""
        del current["results"][0]["subtest"]
        current["results"][0]["key"] = key("", "")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "wpt.json"
            path.write_text(json.dumps(current), encoding="utf-8")
            with self.assertRaises(ValueError) as caught:
                wpt_gate.load_report(path)
        message = str(caught.exception)
        self.assertIn("results[0]", message)
        self.assertIn("subtest", message)

    def test_rebaseline_writes_toml_that_gate_accepts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline_path = root / "baseline.json"
            allowlist_path = root / "allowlist.toml"
            current = report({NAME: "FAIL"})
            wpt_gate.rebaseline(current, baseline_path, allowlist_path)
            saved = wpt_gate.load_baseline(baseline_path)
            allowed = wpt_gate.load_allowlist(allowlist_path)
            self.assertEqual(saved["passed"], 1)
            self.assertEqual(len(allowed), 1)
            self.assertIsNone(wpt_gate.check_gate(current, saved, allowed))

    def test_repository_gate_is_green(self) -> None:
        current = wpt_gate.load_report(ROOT / "docs/status/wpt.json")
        saved = wpt_gate.load_baseline(ROOT / "tests/wpt/baseline.json")
        allowed = wpt_gate.load_allowlist(ROOT / "tests/wpt/allowlist.toml")
        self.assertIsNone(wpt_gate.check_gate(current, saved, allowed))

    def test_repository_report_has_at_least_100_real_passes(self) -> None:
        """M4 floor: every reported PASS comes from the harness; the allowlist stays empty."""
        current = wpt_gate.load_report(ROOT / "docs/status/wpt.json")
        allowed = wpt_gate.load_allowlist(ROOT / "tests/wpt/allowlist.toml")
        self.assertGreaterEqual(current["total"], 100, f"only {current['total']} subtests executed")
        self.assertEqual(current["counts"]["pass"], current["total"], "a non-PASS would need an allowlist")
        self.assertEqual(allowed, [], "allowlist must stay empty")
        self.assertEqual(wpt_gate.canonical_key(TEST, NAME), key())


if __name__ == "__main__":
    unittest.main()
