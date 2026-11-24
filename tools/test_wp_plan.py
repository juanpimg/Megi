"""Tests for the WP planner: concurrency cap and priority ordering."""
import unittest
from tools import wp_plan
from tools.wp_schema import WorkPackage

def make_wp(wid, impact=1, frequency=1):
    return WorkPackage(id=wid, source="wpt", file="crates/x.rs", expected="PASS", actual="FAIL",
        spec="§1", acceptance="test passes", verify_cmd="cargo test x",
        metric="wpt_passed", milestone="M0", impact=impact, frequency=frequency)

class PlannerTests(unittest.TestCase):
    def test_max_ten_in_progress(self):
        wps = [make_wp(f"WP-WPT-{i:03d}") for i in range(25)]
        self.assertEqual(len(wp_plan.select(wps, [], limit=10)), 10)

    def test_priority_is_impact_times_frequency(self):
        low, high = make_wp("WP-WPT-low"), make_wp("WP-WPT-high", 10, 9)
        self.assertEqual(wp_plan.select([low, high], [], limit=1)[0].id, "WP-WPT-high")

    def test_in_progress_are_excluded(self):
        a, b = make_wp("WP-WPT-a"), make_wp("WP-WPT-b")
        self.assertEqual([w.id for w in wp_plan.select([a, b], ["WP-WPT-a"], 10)], ["WP-WPT-b"])

    def test_render_board_has_columns(self):
        board = wp_plan.render_board([make_wp("WP-WPT-x")], ["WP-WPT-x"], [], [])
        for col in ("queued", "in_progress", "review", "done"):
            self.assertIn(col, board)
        self.assertIn("WP-WPT-x", board)

if __name__ == "__main__": unittest.main()
