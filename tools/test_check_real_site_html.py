#!/usr/bin/env python3
"""Unit tests for tools/check_real_site_html.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_real_site_html.py")
_spec = importlib.util.spec_from_file_location("check_real_site_html", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_real_site_html"] = checker
_spec.loader.exec_module(checker)

HTML = """<p class="meta">Modo: <b>snapshot</b> · capturado el 2026-11-26 · PASS: 1 · DEGRADADO: 1 · PÁGINA EN BLANCO: 0 · ERROR: 0 ·</p>
<table><tr><th>Sitio</th><th>Resultado</th></tr>
<tr><td><b>example.com</b><br><span>x</span></td><td style='color:#2ea043'><b>PASS</b></td></tr>
<tr><td><b>rust-lang.org</b><br><span>x</span></td><td style='color:#d29922'><b>DEGRADADO</b></td></tr></table>
"""
REPORT = {
    "mode": "snapshot",
    "results": [
        {"name": "example.com", "result": "PASS"},
        {"name": "rust-lang.org", "result": "DEGRADADO"},
    ],
}


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        status = self.root / "docs/status"
        status.mkdir(parents=True)
        (status / "real-sites.json").write_text(json.dumps(REPORT), encoding="utf-8")
        (status / "real-sites.html").write_text(HTML, encoding="utf-8")

    def test_valid_report_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_missing_summary_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").write_text("<table></table>", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertTrue(any("summary line" in p for p in problems))

    def test_mode_mismatch_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.json").write_text(
            json.dumps({**REPORT, "mode": "live"}), encoding="utf-8"
        )
        self.assertTrue(any("mode 'snapshot' != json 'live'" in p for p in checker.validate(self.root)))

    def test_wrong_count_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").write_text(
            HTML.replace("PASS: 1", "PASS: 2"), encoding="utf-8"
        )
        self.assertTrue(any("PASS=2" in p for p in checker.validate(self.root)))

    def test_missing_row_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").write_text(
            HTML.replace("<b>rust-lang.org</b>", "<b>other</b>"), encoding="utf-8"
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("no row for 'rust-lang.org'" in p for p in problems))

    def test_result_mismatch_is_reported(self) -> None:
        (self.root / "docs/status/real-sites.html").write_text(
            HTML.replace("<b>DEGRADADO</b>", "<b>PASS</b>"), encoding="utf-8"
        )
        self.assertTrue(any("shows 'PASS', json says 'DEGRADADO'" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_report(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("real-site HTML consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("REAL SITE HTML:", result.stderr)


if __name__ == "__main__":
    unittest.main()
