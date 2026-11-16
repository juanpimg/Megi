#!/usr/bin/env python3
"""Unit tests for tools/real_site_check.py (real-site render report)."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("real_site_check.py")
_spec = importlib.util.spec_from_file_location("real_site_check", MODULE_PATH)
real_site_check = importlib.util.module_from_spec(_spec)
sys.modules["real_site_check"] = real_site_check
_spec.loader.exec_module(real_site_check)


class ClassifyTests(unittest.TestCase):
    def test_blank_page_when_no_display_items(self) -> None:
        result = real_site_check.classify({"display_items": 0, "warnings": [], "status": 200})
        self.assertEqual(result, "PÁGINA EN BLANCO")

    def test_degraded_when_warnings(self) -> None:
        result = real_site_check.classify(
            {"display_items": 42, "warnings": ["skipping image"], "status": 200}
        )
        self.assertEqual(result, "DEGRADADO")

    def test_pass_when_clean(self) -> None:
        result = real_site_check.classify(
            {
                "display_items": 42,
                "warnings": [],
                "status": 200,
                "images_failed": 0,
                "scripts_failed": 0,
            }
        )
        self.assertEqual(result, "PASS")

    def test_min_items_degrades(self) -> None:
        result = real_site_check.classify(
            {"display_items": 10, "warnings": [], "status": 200}, {"min_items": 100}
        )
        self.assertEqual(result, "DEGRADADO")


class ReportTests(unittest.TestCase):
    def test_report_links_issues_and_screenshots(self) -> None:
        results = [
            {
                "id": "hn",
                "name": "news.ycombinator.com",
                "source": "live",
                "result": "PÁGINA EN BLANCO",
                "display_items": 0,
                "warnings": ["layout produced no items"],
                "issue": 1011,
                "png": "real-sites/hn.png",
            }
        ]
        html = real_site_check.render_html(results)
        self.assertIn("issues/1011", html)
        self.assertIn("real-sites/hn.png", html)
        self.assertIn("PÁGINA EN BLANCO", html)
        self.assertIn("news.ycombinator.com", html)


if __name__ == "__main__":
    unittest.main()
