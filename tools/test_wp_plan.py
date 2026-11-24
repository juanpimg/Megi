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

def write_queue_wp(root, wid):
    queue = root / "docs/work-packages/queue"
    queue.mkdir(parents=True, exist_ok=True)
    (queue / f"{wid}.toml").write_text(
        f'id = "{wid}"\nsource = "depth"\nfile = "docs/x.md"\n'
        'expected = "ok"\nactual = "pending"\nspec = "s"\nacceptance = "done"\n'
        'verify_cmd = "python3 tools/check_arch.py"\nmetric = "depth_closed"\n'
        'milestone = "M0"\nimpact = 3\nfrequency = 3\nprofile = "light"\n',
        encoding="utf-8",
    )
    return queue

def board_column(board, name):
    marker = f"## {name}\n"
    start = board.index(marker) + len(marker)
    end = board.find("\n## ", start)
    return board[start:] if end == -1 else board[start:end]

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


    def test_cap_accounts_for_active_work(self) -> None:
        wps = [make_wp(f"WP-WPT-{i:03d}") for i in range(25)]
        active = [f"WP-WPT-{i:03d}" for i in range(10)]
        with contextlib.redirect_stdout(io.StringIO()):
            selected = wp_plan.select(wps, active, limit=30)
        self.assertEqual(len(selected), 0)  # 10 active + limit 30 must still respect MAX_IN_PROGRESS
        self.assertEqual(len(wp_plan.select(wps, [], limit=-5)), 0)

    def test_claim_writes_state_and_refuses_when_full(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queue = root / "docs/work-packages/queue"
            queue.mkdir(parents=True)
            for i in range(11):
                wid = f"WP-DEPTH-{i:02d}"
                (queue / f"{wid}.toml").write_text(
                    f'id = "{wid}"\nsource = "depth"\nfile = "docs/x.md"\n'
                    'expected = "ok"\nactual = "pending"\nspec = "s"\nacceptance = "done"\n'
                    'verify_cmd = "python3 tools/check_arch.py"\nmetric = "depth_closed"\n'
                    'milestone = "M0"\nimpact = 3\nfrequency = 3\nprofile = "light"\n',
                    encoding="utf-8",
                )
            for i in range(10):
                self.assertEqual(wp_plan.main(["--claim", f"WP-DEPTH-{i:02d}", "--root", str(root)]), 0)
            self.assertEqual(len(wp_plan.load_state(root)[0]), 10)
            self.assertEqual(wp_plan.main(["--claim", "WP-DEPTH-10", "--root", str(root)]), 1)
            self.assertNotIn("WP-DEPTH-10", wp_plan.load_state(root)[0])

    def test_done_removes_queue_and_records_metric(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queue = root / "docs/work-packages/queue"
            queue.mkdir(parents=True)
            (queue / "WP-DEPTH-00.toml").write_text(
                'id = "WP-DEPTH-00"\nsource = "depth"\nfile = "docs/x.md"\n'
                'expected = "ok"\nactual = "pending"\nspec = "s"\nacceptance = "done"\n'
                'verify_cmd = "python3 tools/check_arch.py"\nmetric = "depth_closed"\n'
                'milestone = "M0"\nimpact = 3\nfrequency = 3\nprofile = "light"\n',
                encoding="utf-8",
            )
            self.assertEqual(
                wp_plan.main(["--claim", "WP-DEPTH-00", "--done", "WP-DEPTH-00",
                              "--metric", "hito M0 cerrado", "--root", str(root)]), 0,
            )
            self.assertFalse((queue / "WP-DEPTH-00.toml").exists())
            closed = wp_plan.load_closed(root)
            self.assertEqual(closed["WP-DEPTH-00"]["metric"], "hito M0 cerrado")

    def test_board_honors_profile_filter(self) -> None:
        light = make_wp("WP-DEPTH-light")
        heavy = make_wp("WP-WPT-heavy")
        object.__setattr__(heavy, "profile", "heavy")
        board = wp_plan.render_board([light, heavy], [], [], [], profile="light")
        self.assertIn("WP-DEPTH-light", board)
        self.assertNotIn("WP-WPT-heavy", board)

    def test_board_profile_filter_does_not_emit_false_refill_warning(self) -> None:
        light = [make_wp(f"WP-DEPTH-light-{i:02d}", impact=2, frequency=3)
                 for i in range(21)]
        heavy = [make_wp(f"WP-WPT-heavy-{i:02d}", impact=3, frequency=3)
                 for i in range(2)]
        for wp in heavy:
            object.__setattr__(wp, "profile", "heavy")
        board = wp_plan.render_board(light + heavy, [], [], [], profile="heavy")
        self.assertNotIn("Cola baja", board)
        queued = board_column(board, "queued")
        for wp in heavy:
            self.assertIn(wp.id, queued)

    def test_board_after_done_shows_closed_wp_in_done_not_queued(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_queue_wp(root, "WP-DEPTH-00")
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(wp_plan.main(["--claim", "WP-DEPTH-00", "--root", str(root)]), 0)
                self.assertEqual(
                    wp_plan.main(["--done", "WP-DEPTH-00", "--metric", "hito M0 cerrado",
                                  "--board", "--root", str(root)]), 0,
                )
            board = (root / "docs/work-packages/BOARD.md").read_text(encoding="utf-8")
            self.assertNotIn("WP-DEPTH-00", board_column(board, "queued"))
            self.assertIn("WP-DEPTH-00", board_column(board, "done"))

    def test_board_after_claim_shows_wp_in_progress_not_queued(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_queue_wp(root, "WP-DEPTH-00")
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(
                    wp_plan.main(["--claim", "WP-DEPTH-00", "--board", "--root", str(root)]), 0,
                )
            board = (root / "docs/work-packages/BOARD.md").read_text(encoding="utf-8")
            self.assertIn("WP-DEPTH-00", board_column(board, "in_progress"))
            self.assertNotIn("WP-DEPTH-00", board_column(board, "queued"))


if __name__ == "__main__": unittest.main()
