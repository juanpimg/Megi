#!/usr/bin/env python3
"""Unit tests for tools/check_real_site_report.py (Python only)."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_real_site_report.py")
_spec = importlib.util.spec_from_file_location("check_real_site_report", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_real_site_report"] = checker
_spec.loader.exec_module(checker)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "real_sites.toml").write_text('[[site]]\nid = "example"\n', encoding="utf-8")
        image = status / "real-sites/example.png"
        image.parent.mkdir()
        image.write_bytes(b"png-bytes")
        (status / "real-sites.json").write_text(
            json.dumps(
                {
                    "mode": "snapshot",
                    "results": [
                        {
                            "id": "example",
                            "result": "PASS",
                            "mode": "snapshot",
                            "time_ms": 10.5,
                            "display_items": 3,
                            "png": "real-sites/example.png",
                            "png_md5": hashlib.md5(b"png-bytes").hexdigest(),
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )

    def _report(self) -> dict:
        return json.loads((self.root / "docs/status/real-sites.json").read_text())

    def _save(self, data: dict) -> None:
        (self.root / "docs/status/real-sites.json").write_text(json.dumps(data), encoding="utf-8")

    def test_valid_report_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/real-sites.json").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_unknown_site_is_reported(self) -> None:
        data = self._report()
        data["results"][0]["id"] = "other"
        self._save(data)
        self.assertTrue(any("unknown site" in p for p in checker.validate(self.root)))

    def test_invalid_result_and_mode_are_reported(self) -> None:
        data = self._report()
        data["results"][0].update(result="MAYBE", mode="unknown")
        self._save(data)
        problems = checker.validate(self.root)
        self.assertTrue(any("invalid result" in p for p in problems))
        self.assertTrue(any("invalid mode" in p for p in problems))

    def test_negative_time_and_items_are_reported(self) -> None:
        data = self._report()
        data["results"][0].update(time_ms=-1, display_items=-2)
        self._save(data)
        problems = checker.validate(self.root)
        self.assertTrue(any("time_ms" in p for p in problems))
        self.assertTrue(any("display_items" in p for p in problems))

    def test_missing_png_is_reported(self) -> None:
        data = self._report()
        data["results"][0]["png"] = "real-sites/absent.png"
        self._save(data)
        self.assertTrue(any("missing capture" in p for p in checker.validate(self.root)))

    def test_png_md5_mismatch_is_reported(self) -> None:
        data = self._report()
        data["results"][0]["png_md5"] = "0" * 32
        self._save(data)
        self.assertTrue(any("png_md5 mismatch" in p for p in checker.validate(self.root)))

    def test_site_missing_from_report_is_reported(self) -> None:
        self._save({"mode": "snapshot", "results": []})
        self.assertIn("example: missing from real-sites.json", checker.validate(self.root))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_report(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("real-site report consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("REAL SITE REPORT:", result.stderr)


if __name__ == "__main__":
    unittest.main()
