"""Tests for the generated WPT headline, manifest block, badge and report parser."""

from __future__ import annotations

import unittest

from tools import update_progress, wpt_report


def report(counts: dict, total: int, dirs: list[str]) -> dict:
    return {
        "total": total,
        "counts": counts,
        "results": [{"test": f"tests/wpt/{d}/x.html", "subtest": f"s{i}", "status": "PASS"}
                    for i, d in enumerate(dirs)],
        "run_info": {"dirs": dirs},
        "epoch": {"wpt_revision": "ab2ef95d3a7c512a6e7321688c80e428086af426"},
    }


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


if __name__ == "__main__":
    unittest.main()
