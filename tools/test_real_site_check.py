#!/usr/bin/env python3
"""Unit tests for tools/real_site_check.py (real-site render report)."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from unittest import mock
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


class BlankDomGateTests(unittest.TestCase):
    """#2269: JS may run cleanly yet leave an empty post-script DOM (SPA blank)."""

    def test_empty_post_script_dom_is_blank_even_with_display_items(self) -> None:
        result = real_site_check.classify(
            {
                "display_items": 2,
                "element_count": 0,
                "body_text": "",
                "status": 200,
                "warnings": [],
            }
        )
        self.assertEqual(result, "PÁGINA EN BLANCO")

    def test_hydrated_dom_still_passes(self) -> None:
        result = real_site_check.classify(
            {
                "display_items": 40,
                "element_count": 12,
                "body_text": "hydrated card",
                "status": 200,
                "warnings": [],
            }
        )
        self.assertEqual(result, "PASS")

    def test_allow_blank_site_may_have_empty_dom(self) -> None:
        result = real_site_check.classify(
            {
                "display_items": 2,
                "element_count": 0,
                "body_text": "",
                "status": 200,
                "warnings": [],
            },
            {"allow_blank": True},
        )
        self.assertEqual(result, "PASS")

    def test_result_carries_element_count_and_body_text(self) -> None:
        result = real_site_check._result(
            {"id": "x"},
            "PASS",
            {"display_items": 3, "element_count": 12, "body_text": "hydrated card"},
            "",
            Path("x.png"),
            "snap",
        )
        self.assertEqual(result["element_count"], 12)
        self.assertEqual(result["body_text"], "hydrated card")

    def test_result_defaults_element_count_and_body_text(self) -> None:
        result = real_site_check._result(
            {"id": "x"}, "PASS", {"display_items": 3}, "", Path("x.png"), "snap"
        )
        self.assertEqual(result["element_count"], 0)
        self.assertEqual(result["body_text"], "")


class PathTests(unittest.TestCase):
    def test_png_path_is_relative_to_report(self) -> None:
        from pathlib import Path

        self.assertEqual(real_site_check.png_relative("hn"), Path("real-sites/hn.png"))
        result = real_site_check._result(
            {"id": "hn", "name": "HN"},
            "PASS",
            {"display_items": 1},
            "",
            real_site_check.png_relative("hn"),
            "https://news.ycombinator.com",
        )
        self.assertEqual(result["png"], "real-sites/hn.png")
        html = real_site_check.render_html([result])
        self.assertIn("real-sites/hn.png", html)
        self.assertNotIn("docs/status/docs/status", html)


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


class FailureReasonTests(unittest.TestCase):
    def test_blank_page_is_a_hard_failure(self) -> None:
        # #1681: classify() emits "PÁGINA EN BLANCO"; failure_reasons() must treat that exact
        # classification as a hard failure (the old "PÁGINA EN BLANK" string was never emitted).
        site = {"id": "blank", "name": "blank.example", "min_items": 1}
        result = {
            "id": "blank",
            "name": "blank.example",
            "result": real_site_check.classify({"display_items": 0, "warnings": [], "status": 200}),
            "display_items": 0,
        }
        reasons = real_site_check.failure_reasons([site], [result])
        self.assertTrue(reasons, "a blank page must be reported as a hard failure")
        self.assertIn("PÁGINA EN BLANCO", reasons[0])


class RenderCommandTests(unittest.TestCase):
    def test_render_one_invokes_real_megi_render_package(self) -> None:
        # #1831: the package was renamed to `megi-render` in the modular split;
        # invoking `-p render` makes every site fail with "package not found".
        fake = mock.Mock(returncode=0, stdout='{"display_items": 1, "warnings": [], "status": 200}', stderr="")
        with tempfile.TemporaryDirectory() as root:
            with mock.patch.object(real_site_check.subprocess, "run", return_value=fake) as run:
                result = real_site_check.render_one(
                    Path(root), {"id": "hn", "name": "HN", "snapshot": "hn.html"}, live=False
                )
        command = run.call_args.args[0]
        self.assertEqual(command[command.index("-p") + 1], "megi-render")
        self.assertEqual(result["result"], "PASS")


class RobustnessTests(unittest.TestCase):
    def test_non_numeric_failure_counts_mark_site_degraded(self) -> None:
        self.assertEqual(
            real_site_check.classify({"display_items": 10, "images_failed": "n/a"}),
            "DEGRADADO",
        )
        self.assertEqual(
            real_site_check.classify({"display_items": 10, "scripts_failed": "n/a"}),
            "DEGRADADO",
        )
        self.assertEqual(
            real_site_check.classify({"display_items": 10, "images_failed": None, "scripts_failed": None}),
            "PASS",
        )

    def test_result_tolerates_non_dict_and_non_numeric_timings_us(self) -> None:
        res_int = real_site_check._result(
            {"id": "x"}, "PASS", {"timings_us": 123}, "", Path("x.png"), "snap"
        )
        self.assertEqual(res_int["time_ms"], 0.0)
        res_str = real_site_check._result(
            {"id": "x"}, "PASS", {"timings_us": {"total": "n/a"}}, "", Path("x.png"), "snap"
        )
        self.assertEqual(res_str["time_ms"], 0.0)


class ConsoleErrorTests(unittest.TestCase):
    """M1: console_errors from `render --json` must reach the report and the verdict."""

    def test_console_error_degrades_an_otherwise_clean_site(self) -> None:
        result = real_site_check.classify(
            {
                "display_items": 42,
                "status": 200,
                "warnings": [],
                "images_failed": 0,
                "scripts_failed": 0,
                "console_errors": 1,
            }
        )
        self.assertEqual(result, "DEGRADADO")

    def test_result_carries_console_errors(self) -> None:
        result = real_site_check._result(
            {"id": "x"},
            "DEGRADADO",
            {"display_items": 3, "console_errors": 2},
            "",
            Path("x.png"),
            "snap",
        )
        self.assertEqual(result["console_errors"], 2)

    def test_result_tolerates_missing_console_errors(self) -> None:
        result = real_site_check._result(
            {"id": "x"}, "PASS", {"display_items": 3}, "", Path("x.png"), "snap"
        )
        self.assertEqual(result["console_errors"], 0)

    def test_render_html_displays_console_errors(self) -> None:
        item = {
            "id": "x",
            "name": "x.example",
            "result": "DEGRADADO",
            "display_items": 3,
            "warnings": [],
            "console_errors": 2,
            "issue": 0,
            "png": "real-sites/x.png",
        }
        html = real_site_check.render_html([item])
        self.assertIn("console_errors", html)
        self.assertIn(">2<", html)

    def test_render_one_propagates_console_errors(self) -> None:
        # The render CLI already emits `console_errors` in its JSON stats; the report
        # must carry the value through instead of silently dropping it.
        fake = mock.Mock(
            returncode=0,
            stdout='{"display_items": 5, "warnings": [], "status": 200, "console_errors": 1}',
            stderr="",
        )
        with tempfile.TemporaryDirectory() as root:
            with mock.patch.object(real_site_check.subprocess, "run", return_value=fake):
                result = real_site_check.render_one(
                    Path(root), {"id": "x", "name": "X", "snapshot": "x.html"}, live=False
                )
        self.assertEqual(result["console_errors"], 1)
        self.assertEqual(result["result"], "DEGRADADO")


if __name__ == "__main__":
    unittest.main()
