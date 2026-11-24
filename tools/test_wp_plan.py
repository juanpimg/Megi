"""Tests for the WP planner: concurrency cap, priority, reflow and machine profile."""
import contextlib
import io
import tempfile
import unittest
from pathlib import Path

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

    def test_profile_filter_selects_only_that_machine(self) -> None:
        light = make_wp("WP-DEPTH-light")
        heavy = make_wp("WP-WPT-heavy")
        object.__setattr__(light, "profile", "light")
        object.__setattr__(heavy, "profile", "heavy")
        selected = wp_plan.select([heavy, light], [], limit=10, profile="light")
        self.assertEqual([wp.id for wp in selected], ["WP-DEPTH-light"])

    def test_refill_when_queue_below_twenty(self) -> None:
        self.assertTrue(wp_plan.needs_refill(19))
        self.assertFalse(wp_plan.needs_refill(20))

    def test_planner_never_emits_merge_commands(self) -> None:
        source = Path(wp_plan.__file__).read_text(encoding="utf-8")
        self.assertNotIn("git merge", source)
        self.assertNotIn("gh pr merge", source)

    def test_cli_board_and_select_use_real_queue(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queue = root / "docs/work-packages/queue"
            queue.mkdir(parents=True)
            (queue / "WP-DEPTH-test.toml").write_text(
                'id = "WP-DEPTH-test"\nsource = "depth"\nfile = "docs/x.md"\n'
                'expected = "ok"\nactual = "pending"\nspec = "spec v2 §4.3"\n'
                'acceptance = "done"\nverify_cmd = "python3 tools/check_arch.py"\n'
                'metric = "depth_closed"\nmilestone = "M0"\nimpact = 4\nfrequency = 2\n',
                encoding="utf-8",
            )
            with contextlib.redirect_stdout(io.StringIO()):
                wp_plan.main(["--board", "--root", str(root)])
            board = (root / "docs/work-packages/BOARD.md").read_text(encoding="utf-8")
            self.assertIn("WP-DEPTH-test", board)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                wp_plan.main(["--select", "--root", str(root), "--profile", "light"])
            self.assertIn("WP-DEPTH-test", output.getvalue())


if __name__ == "__main__": unittest.main()
