#!/usr/bin/env python3
"""Unit tests for tools/check_real_site_md5.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_real_site_md5.py")
_spec = importlib.util.spec_from_file_location("check_real_site_md5", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_real_site_md5"] = checker
_spec.loader.exec_module(checker)

MD5 = "a" * 32
HTML = (
    "<tr><td><b>example.com</b><br><span>x</span></td><td><b>PASS</b></td>"
    f"<td><a href='real-sites/example.png'><img src='real-sites/example.png' alt='captura de example.com'>"
    f"</a><br><span class='meta'>md5 <code title='md5 completo'>{MD5}</code></span></td></tr>"
)
REPORT = {"results": [{"name": "example.com", "png_md5": MD5}]}


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "real-sites.json").write_text(json.dumps(REPORT), encoding="utf-8")
        (status / "real-sites.html").write_text(HTML, encoding="utf-8")

    def test_valid_annotation_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_md5_mismatch_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.json").write_text(
            json.dumps({"results": [{"name": "example.com", "png_md5": "b" * 32}]}),
            encoding="utf-8",
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("html md5" in p for p in problems))

    def test_alt_mismatch_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").write_text(
            HTML.replace("alt='captura de example.com'", "alt='captura de other'"), encoding="utf-8"
        )
        self.assertTrue(any("image alt" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_report(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("real-site md5 annotations consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("REAL SITE MD5:", result.stderr)


if __name__ == "__main__":
    unittest.main()
