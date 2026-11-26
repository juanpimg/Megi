#!/usr/bin/env python3
"""Unit tests for tools/check_wpt_artifacts.py (Python only)."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wpt_artifacts.py")
_spec = importlib.util.spec_from_file_location("check_wpt_artifacts", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wpt_artifacts"] = checker
_spec.loader.exec_module(checker)

SHA = "a" * 40
MANIFEST_HASH = hashlib.sha256(b"manifest").hexdigest()


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self._write("tests/wpt/WPT-REVISION.json", {"revision": SHA})
        self._write_file("tests/wpt/MANIFEST.json", b"manifest")
        self._write(
            "tests/wpt/baseline.json",
            {
                "epoch": {"wpt_revision": SHA, "manifest_sha256": MANIFEST_HASH},
                "passed": 2,
                "total": 2,
                "allowlist": [],
            },
        )
        self._write(
            "docs/status/wpt.json",
            {
                "schema": "megi-wpt-report/1",
                "epoch": {"wpt_revision": SHA, "manifest_sha256": MANIFEST_HASH},
                "counts": {"pass": 2, "fail": 0},
            },
        )
        self._write_file(
            "docs/status/progress.toml",
            (
                "[wpt]\n"
                f'revision = "{SHA}"\n'
                f'manifest_sha256 = "{MANIFEST_HASH}"\n'
                "passed = 2\ntotal = 2\nallowlist = 0\n"
            ).encode(),
        )

    def _write(self, relative: str, data: object) -> None:
        self._write_file(relative, (json.dumps(data) + "\n").encode())

    def _write_file(self, relative: str, data: bytes) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def test_valid_artifacts_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_artifact_is_reported(self) -> None:
        (self.root / "tests/wpt/MANIFEST.json").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing artifact"))

    def test_bad_revision_is_reported(self) -> None:
        self._write("tests/wpt/WPT-REVISION.json", {"revision": "nope"})
        problems = checker.validate(self.root)
        self.assertTrue(any("invalid revision" in p for p in problems))

    def test_baseline_sha_mismatch_is_reported(self) -> None:
        self._write_file("tests/wpt/MANIFEST.json", b"changed")
        problems = checker.validate(self.root)
        self.assertTrue(any("manifest_sha256 does not match" in p for p in problems))

    def test_report_epoch_mismatch_is_reported(self) -> None:
        self._write(
            "docs/status/wpt.json",
            {
                "schema": "megi-wpt-report/1",
                "epoch": {"wpt_revision": "b" * 40, "manifest_sha256": MANIFEST_HASH},
                "counts": {"pass": 2, "fail": 0},
            },
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("wpt.json epoch differs" in p for p in problems))

    def test_counts_mismatch_is_reported(self) -> None:
        self._write(
            "docs/status/wpt.json",
            {
                "schema": "megi-wpt-report/1",
                "epoch": {"wpt_revision": SHA, "manifest_sha256": MANIFEST_HASH},
                "counts": {"pass": 1, "fail": 0},
            },
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("counts do not add up" in p for p in problems))

    def test_progress_mismatch_is_reported(self) -> None:
        self._write_file(
            "docs/status/progress.toml",
            (
                "[wpt]\n"
                f'revision = "{SHA}"\n'
                f'manifest_sha256 = "{MANIFEST_HASH}"\n'
                "passed = 9\ntotal = 2\nallowlist = 0\n"
            ).encode(),
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("[wpt].passed=9" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_artifacts(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WPT artifacts consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WPT ARTIFACT:", result.stderr)


if __name__ == "__main__":
    unittest.main()
