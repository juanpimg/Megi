#!/usr/bin/env python3
"""Unit tests for tools/check_test262_artifacts.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_test262_artifacts.py")
_spec = importlib.util.spec_from_file_location("check_test262_artifacts", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_test262_artifacts"] = checker
_spec.loader.exec_module(checker)

SHA = "a" * 64
SCOPE = "subset"
MANIFEST = (
    f'[test262]\nmanifest_sha256 = "{SHA}"\nscope = "{SCOPE}"\n'
    "passed = 2\ntotal = 2\nallowlist = 0\n"
)
REPORT = {
    "schema": "megi-test262-report/1",
    "epoch": {"manifest_sha256": SHA, "scope": SCOPE},
    "counts": {"pass": 2, "fail": 0},
    "total": 2,
    "allowlist": 0,
    "headline": "Test262 aaaa — 2/2 cases passed",
}


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "test262.json").write_text(json.dumps(REPORT), encoding="utf-8")
        (status / "progress.toml").write_text(MANIFEST, encoding="utf-8")

    def test_valid_artifacts_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/test262.json").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_wrong_schema_is_reported(self) -> None:
        report = dict(REPORT, schema="other/1")
        (self.root / "docs/status/test262.json").write_text(json.dumps(report), encoding="utf-8")
        self.assertTrue(any("unexpected schema" in p for p in checker.validate(self.root)))

    def test_epoch_mismatch_is_reported(self) -> None:
        report = dict(REPORT, epoch={"manifest_sha256": "b" * 64, "scope": SCOPE})
        (self.root / "docs/status/test262.json").write_text(json.dumps(report), encoding="utf-8")
        self.assertTrue(any("epoch.manifest_sha256" in p for p in checker.validate(self.root)))

    def test_count_mismatch_is_reported(self) -> None:
        report = dict(REPORT, counts={"pass": 1, "fail": 0})
        (self.root / "docs/status/test262.json").write_text(json.dumps(report), encoding="utf-8")
        self.assertTrue(any("disagrees with progress.toml" in p for p in checker.validate(self.root)))

    def test_counts_sum_mismatch_is_reported(self) -> None:
        report = dict(REPORT, counts={"pass": 2, "fail": 1})
        (self.root / "docs/status/test262.json").write_text(json.dumps(report), encoding="utf-8")
        self.assertTrue(any("do not add up" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_artifacts(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Test262 artifacts consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("TEST262 ARTIFACT:", result.stderr)


if __name__ == "__main__":
    unittest.main()
