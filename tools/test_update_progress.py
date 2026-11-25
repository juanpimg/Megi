"""Tests for the generated WPT headline, manifest block, badge and report parser."""

from __future__ import annotations

import contextlib
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import progress_stats, update_progress, wpt_report


def report(counts: dict, total: int, dirs: list[str]) -> dict:
    return {
        "schema": "megi-wpt-report/1",
        "total": total,
        "counts": counts,
        "results": [{"test": f"tests/wpt/{d}/x.html", "subtest": f"s{i}", "status": "PASS"}
                    for i, d in enumerate(dirs)],
        "run_info": {"dirs": dirs},
        "epoch": {
            "wpt_revision": "ab2ef95d3a7c512a6e7321688c80e428086af426",
            "manifest_sha256": "0" * 64,
        },
    }


def write_report(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


class UpdateProgressWptTest(unittest.TestCase):
    def test_plan_headline_exact(self) -> None:
        r = report(
            {"pass": 3, "fail": 2, "timeout": 1, "notrun": 1, "error": 1},
            8,
            [f"tests/wpt/d{i}" for i in range(6)],
        )
        self.assertEqual(
            update_progress.render_wpt_headline(r, {"wpt_revision": "abc1234"}),
            "WPT abc1234 — 3/8 subtests pasados "
            "(dirs: 6; 2 FAIL, 1 TIMEOUT, 1 NOTRUN, 1 harness_error)",
        )

    def test_clean_headline_has_no_distribution(self) -> None:
        r = report({"pass": 30, "fail": 0, "timeout": 0, "notrun": 0, "error": 0},
                   30, ["tests/wpt/css/css-syntax"])
        self.assertEqual(
            update_progress.render_wpt_headline(r, r["epoch"]),
            "WPT ab2ef95 — 30/30 subtests pasados (dirs: 1)",
        )

    def test_update_manifest_rewrites_only_wpt_block(self) -> None:
        text = ("[stats]\nhtml = 1\n\n[gate]\nstatus = old\n\n"
                "[wpt]\nrevision = stale\npassed = 1\ntotal = 2\n\n"
                "[[phase]]\nname = M0\n")
        stats = {"tests": 2, "tests_commit": "abc", "tests_date": "2026-11-24",
                 "crates": 1, "rust_lines": 1, "unsafe_forbidden": 1}
        gate = {"status": "PASS", "verified_commit": "abc", "tokenizer": "7050/7050"}
        result = update_progress.update_manifest(
            text, stats, gate,
            {"revision": "abc", "passed": "30", "total": "30", "dirs": "1",
             "distribution": "PASS=30 FAIL=0"},
        )
        self.assertIn("tests = 2", result)
        self.assertIn('tokenizer = "7050/7050"', result)
        self.assertIn('revision = "abc"', result)
        self.assertNotIn("stale", result)
        self.assertIn("[[phase]]", result)

    def test_render_badges_includes_wpt(self) -> None:
        stats = {"tests": 1, "unsafe_forbidden": 1, "crates": 1}
        badges = update_progress.render_badges(
            {"stats": stats, "gate": {"status": "PASS"},
             "wpt": {"passed": 30, "total": 30, "status": "PASS"}}
        )
        self.assertIn(">30/30</text>", badges["wpt"])

    def test_parse_summary_reads_real_shape(self) -> None:
        output = ("--- WPT CSS Syntax Suites (tests/wpt/css/css-syntax/*.html) ---\n"
                  "  [PASS] a.html: s1\n  [FAIL] a.html: s2\n"
                  "CSS Syntax Total: 1 / 2 tests passed\n")
        results, passed, total = wpt_report.parse_summary(output)
        self.assertEqual((passed, total), (1, 2))
        self.assertEqual([s for _, s in results], ["PASS", "FAIL"])

    def test_parse_summary_rejects_missing_section(self) -> None:
        with self.assertRaises(ValueError):
            wpt_report.parse_summary("no WPT section here\n")

    def test_load_report_rejects_incomplete_shape(self) -> None:
        # A report with only a schema must fail with a ValueError naming the first
        # missing key, never a KeyError/TypeError later in wpt_fields().
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "wpt.json"
            write_report(path, {"schema": "megi-wpt-report/1"})
            with self.assertRaises(ValueError) as caught:
                wpt_report.load_report(path)
        self.assertIn("epoch", str(caught.exception))

    def test_load_report_rejects_incomplete_epoch(self) -> None:
        # The test fixture itself must carry manifest_sha256; a report without it
        # used to reach wpt_fields() and only fail there with a KeyError.
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "wpt.json"
            data = report({"pass": 1, "fail": 0, "timeout": 0, "notrun": 0, "error": 0}, 1,
                          ["tests/wpt/css/css-syntax"])
            del data["epoch"]["manifest_sha256"]
            write_report(path, data)
            with self.assertRaises(ValueError) as caught:
                wpt_report.load_report(path)
        self.assertIn("manifest_sha256", str(caught.exception))

    def test_build_artifacts_reports_incomplete_wpt_report(self) -> None:
        # build_artifacts(no_gate=True) promises "error: WPT report: ..." instead of
        # a traceback when docs/status/wpt.json is incomplete (#2290).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            status = root / update_progress.STATUS_DIR
            status.mkdir(parents=True)
            write_report(status / "wpt.json", {"schema": "megi-wpt-report/1"})
            (status / "progress.toml").write_text(
                '[stats]\ntests = 0\n\n[gate]\nstatus = "PASS"\n', encoding="utf-8"
            )
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                with self.assertRaises(SystemExit) as caught:
                    update_progress.build_artifacts(root, no_count=True, no_gate=True)
        self.assertEqual(
            caught.exception.code,
            f"error: WPT report: {status / 'wpt.json'}: report needs epoch (a dict)",
        )
        self.assertNotIn("Traceback", stderr.getvalue())


class FakeCache:
    """In-memory stand-in for the on-disk test-count cache."""

    def __init__(self) -> None:
        self.content: dict | None = None
        self.parent = mock.MagicMock()

    def is_file(self) -> bool:
        return False

    def write_text(self, data: str, encoding: str = "utf-8") -> None:
        self.content = json.loads(data)


class CountTestsCacheTest(unittest.TestCase):
    """The count cache must only be stamped on a clean tree at HEAD (#1394)."""

    def _count(self, status_output: str) -> tuple[tuple[int, bool], FakeCache]:
        cache = FakeCache()

        def fake_run(root: Path, args: list[str], timeout: int = 600) -> subprocess.CompletedProcess[str]:
            if args[:2] == ["git", "status"]:
                return subprocess.CompletedProcess(args, 0, status_output, "")
            if args[0] == "cargo":
                return subprocess.CompletedProcess(args, 0, "a: test\nb: test\n", "")
            return subprocess.CompletedProcess(args, 0, "", "")

        with mock.patch.object(progress_stats, "run", side_effect=fake_run):
            result = progress_stats.count_tests(
                Path("/repo"), cache, head="abc1234", manifest_tests=99, no_count=False
            )
        return result, cache

    def test_dirty_tree_does_not_write_cache(self) -> None:
        (count, measured), cache = self._count(" M crates/x/tests/x.rs\n")
        self.assertEqual((count, measured), (2, True))
        self.assertIsNone(cache.content)

    def test_clean_tree_writes_cache(self) -> None:
        (count, measured), cache = self._count("")
        self.assertEqual((count, measured), (2, True))
        self.assertEqual(cache.content, {"head": "abc1234", "tests": 2})


if __name__ == "__main__":
    unittest.main()
