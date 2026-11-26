#!/usr/bin/env python3
"""Unit tests for tools/check_wpt_manifest_files.py (Python only)."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wpt_manifest_files.py")
_spec = importlib.util.spec_from_file_location("check_wpt_manifest_files", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wpt_manifest_files"] = checker
_spec.loader.exec_module(checker)

DATA = b"<p>test</p>\n"


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        fixture_dir = self.root / "tests/wpt/dom"
        fixture_dir.mkdir(parents=True)
        (fixture_dir / "a.html").write_bytes(DATA)
        (self.root / "tests/wpt/MANIFEST.json").write_text(
            json.dumps([
                {
                    "path": "dom/a.html",
                    "sha256": hashlib.sha256(DATA).hexdigest(),
                    "bytes": len(DATA),
                    "subtest_count_expected": 1,
                }
            ]),
            encoding="utf-8",
        )

    def _entries(self) -> list[dict]:
        return json.loads((self.root / "tests/wpt/MANIFEST.json").read_text())

    def _save(self, entries: list[dict]) -> None:
        (self.root / "tests/wpt/MANIFEST.json").write_text(json.dumps(entries), encoding="utf-8")

    def test_valid_manifest_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_manifest_is_reported(self) -> None:
        (self.root / "tests/wpt/MANIFEST.json").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_fixture_is_reported(self) -> None:
        (self.root / "tests/wpt/dom/a.html").unlink()
        self.assertIn("dom/a.html: missing fixture", checker.validate(self.root))

    def test_sha_mismatch_is_reported(self) -> None:
        entries = self._entries()
        entries[0]["sha256"] = "0" * 64
        self._save(entries)
        self.assertIn("dom/a.html: sha256 mismatch", checker.validate(self.root))

    def test_byte_size_mismatch_is_reported(self) -> None:
        entries = self._entries()
        entries[0]["bytes"] += 1
        self._save(entries)
        self.assertIn("dom/a.html: byte size mismatch", checker.validate(self.root))

    def test_unsafe_path_is_reported(self) -> None:
        entries = self._entries()
        entries[0]["path"] = "../secret.html"
        self._save(entries)
        self.assertTrue(any("unsafe fixture path" in p for p in checker.validate(self.root)))

    def test_zero_subtests_are_allowed_for_support_files(self) -> None:
        entries = self._entries()
        entries[0]["subtest_count_expected"] = 0
        self._save(entries)
        self.assertEqual(checker.validate(self.root), [])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_manifest(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WPT fixtures OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WPT FIXTURE:", result.stderr)


if __name__ == "__main__":
    unittest.main()
