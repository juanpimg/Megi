#!/usr/bin/env python3
"""Unit tests for tools/real_site_check.py (real-site render report)."""

from __future__ import annotations

import hashlib
import importlib.util
import json
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

    def test_min_items_nonnumeric_sentinel_is_ignored(self) -> None:
        # #2202 / #2290: a non-numeric sentinel must degrade gracefully, not crash.
        result = real_site_check.classify(
            {"display_items": 10, "warnings": [], "status": 200}, {"min_items": "n/a"}
        )
        self.assertEqual(result, "PASS")

    def test_min_items_none_is_ignored(self) -> None:
        result = real_site_check.classify(
            {"display_items": 10, "warnings": [], "status": 200}, {"min_items": None}
        )
        self.assertEqual(result, "PASS")


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


class Md5EvidenceTests(unittest.TestCase):
    """M2-MD5-EVIDENCE: every published capture must carry stable md5 evidence."""

    @staticmethod
    def _png_bytes() -> bytes:
        return b"\x89PNG\r\n\x1a\n" + b"fixture-payload"

    def test_png_md5_is_hex_digest_of_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.png"
            path.write_bytes(self._png_bytes())
            expected = hashlib.md5(self._png_bytes()).hexdigest()
            self.assertEqual(real_site_check.png_md5(path), expected)

    def test_png_md5_missing_file_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(real_site_check.png_md5(Path(tmp) / "missing.png"), "")

    def test_result_carries_png_md5(self) -> None:
        digest = "0123456789abcdef0123456789abcdef"
        result = real_site_check._result(
            {"id": "x"},
            "PASS",
            {"display_items": 3},
            "",
            Path("real-sites/x.png"),
            "snap",
            png_md5=digest,
        )
        self.assertEqual(result["png_md5"], digest)

    def test_result_defaults_png_md5_to_empty(self) -> None:
        result = real_site_check._result(
            {"id": "x"}, "PASS", {"display_items": 3}, "", Path("real-sites/x.png"), "snap"
        )
        self.assertEqual(result["png_md5"], "")

    def test_render_one_propagates_png_md5(self) -> None:
        fake = mock.Mock(
            returncode=0,
            stdout='{"display_items": 1, "warnings": [], "status": 200}',
            stderr="",
        )
        with tempfile.TemporaryDirectory() as root:
            png_path = Path(root) / "docs/status/real-sites/x.png"
            png_path.parent.mkdir(parents=True, exist_ok=True)
            png_path.write_bytes(self._png_bytes())
            with mock.patch.object(real_site_check.subprocess, "run", return_value=fake):
                result = real_site_check.render_one(
                    Path(root), {"id": "x", "name": "X", "snapshot": "x.html"}, live=False
                )
        self.assertEqual(result["png_md5"], hashlib.md5(self._png_bytes()).hexdigest())

    def test_render_html_shows_png_md5(self) -> None:
        item = {
            "id": "x",
            "name": "x.example",
            "result": "DEGRADADO",
            "display_items": 3,
            "warnings": [],
            "console_errors": 0,
            "issue": 0,
            "png": "real-sites/x.png",
            "png_md5": "abcdef0123456789abcdef0123456789",
        }
        html = real_site_check.render_html([item])
        self.assertIn("md5", html)
        self.assertIn(item["png_md5"][:8], html)


class RefreshHashesTests(unittest.TestCase):
    """--refresh-hashes republishes evidence from existing PNGs, never via cargo."""

    @staticmethod
    def _seed(root: str) -> tuple[dict, bytes]:
        status = Path(root) / "docs/status"
        (status / "real-sites").mkdir(parents=True, exist_ok=True)
        png_bytes = b"\x89PNG\r\n\x1a\n" + b"refresh-fixture"
        (status / "real-sites/x.png").write_bytes(png_bytes)
        result = {
            "id": "x",
            "name": "x.example",
            "source": "https://x.example",
            "mode": "live",
            "result": "DEGRADADO",
            "display_items": 7,
            "status": 200,
            "warnings": ["performance budget: total 9 > 3s"],
            "console_errors": 0,
            "images_failed": 0,
            "time_ms": 42.0,
            "issue": 1011,
            "error": "",
            "png": "real-sites/x.png",
        }
        payload = {"mode": "live", "generated_at": "2026-11-24", "results": [result]}
        (status / "real-sites.json").write_text(
            json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        return payload, png_bytes

    @staticmethod
    def _no_cargo():
        return mock.patch.object(
            real_site_check.subprocess,
            "run",
            side_effect=AssertionError("cargo must not run in --refresh-hashes"),
        )

    def test_refresh_hashes_rewrites_json_without_cargo(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            payload, png_bytes = self._seed(root)
            with self._no_cargo():
                rc = real_site_check.main(["--refresh-hashes", "--root", root])
            self.assertEqual(rc, 0)
            updated = json.loads(
                (Path(root) / "docs/status/real-sites.json").read_text(encoding="utf-8")
            )
            self.assertEqual(updated["mode"], payload["mode"])
            self.assertEqual(updated["generated_at"], payload["generated_at"])
            self.assertEqual(len(updated["results"]), 1)
            new_result = dict(updated["results"][0])
            self.assertEqual(new_result.pop("png_md5"), hashlib.md5(png_bytes).hexdigest())
            self.assertEqual(new_result, payload["results"][0])

    def test_refresh_hashes_writes_html_with_hash(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            _, png_bytes = self._seed(root)
            with self._no_cargo():
                real_site_check.main(["--refresh-hashes", "--root", root])
            page = (Path(root) / "docs/status/real-sites.html").read_text(encoding="utf-8")
            self.assertIn(hashlib.md5(png_bytes).hexdigest()[:8], page)

    def test_refresh_hashes_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            self._seed(root)
            paths = [
                Path(root) / "docs/status/real-sites.json",
                Path(root) / "docs/status/real-sites.html",
            ]
            with self._no_cargo():
                first = real_site_check.main(["--refresh-hashes", "--root", root])
                snapshots = [path.read_bytes() for path in paths]
                second = real_site_check.main(["--refresh-hashes", "--root", root])
            self.assertEqual((first, second), (0, 0))
            self.assertEqual([path.read_bytes() for path in paths], snapshots)


if __name__ == "__main__":
    unittest.main()
