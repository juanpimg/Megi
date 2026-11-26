#!/usr/bin/env python3
"""Unit tests for tools/visual_parity_report.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("visual_parity_report.py")
_spec = importlib.util.spec_from_file_location("visual_parity_report", MODULE_PATH)
report_tool = importlib.util.module_from_spec(_spec)
sys.modules["visual_parity_report"] = report_tool
_spec.loader.exec_module(report_tool)


class DiscoverTests(unittest.TestCase):
    def test_finds_pairs_and_respects_site_filter(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for site in ("example", "rust"):
                (root / f"{site}-megi.png").write_bytes(b"x")
                (root / f"{site}-chrome.png").write_bytes(b"y")
            pairs = report_tool.discover(root)
            self.assertEqual([site for site, _, _ in pairs], ["example", "rust"])
            self.assertEqual(
                [site for site, _, _ in report_tool.discover(root, ["rust"])], ["rust"]
            )

    def test_missing_pair_is_a_typed_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "example-megi.png").write_bytes(b"x")
            with self.assertRaises(ValueError):
                report_tool.discover(root)

    def test_empty_directory_is_a_typed_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                report_tool.discover(Path(tmp))


class ReportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def _pair(self, site: str, left: tuple[int, int, int], right: tuple[int, int, int]) -> None:
        from PIL import Image

        Image.new("RGB", (4, 4), left).save(self.root / f"{site}-megi.png")
        Image.new("RGB", (4, 4), right).save(self.root / f"{site}-chrome.png")

    def test_build_report_and_markdown(self) -> None:
        self._pair("same", "white", "white")
        self._pair("diff", "white", "black")
        report = report_tool.build_report(self.root)
        self.assertEqual(report["same"]["changed_pixels"], 0)
        self.assertEqual(report["diff"]["changed_pixels"], 16)
        table = report_tool.markdown(report)
        self.assertIn("| same | 0 | 0.0 |", table)
        self.assertIn("| diff | 16 |", table)

    def test_cli_writes_json_and_markdown(self) -> None:
        self._pair("same", "white", "white")
        out_json, out_md = self.root / "r.json", self.root / "r.md"
        result = subprocess.run(
            [
                sys.executable,
                str(MODULE_PATH),
                str(self.root),
                "--json",
                str(out_json),
                "--markdown",
                str(out_md),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(out_json.read_text())["same"]["changed_pixels"], 0)
        self.assertIn("| same |", out_md.read_text())


if __name__ == "__main__":
    unittest.main()
