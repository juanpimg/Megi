"""Tests for the WP refill workflow and the real-throughput dashboard."""
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools import wp_dashboard


class DashboardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _git(self, *args: str) -> None:
        subprocess.run(["git", "-C", str(self.root), *args], check=True,
                       capture_output=True, text=True)

    def test_workflow_runs_generators(self) -> None:
        workflow = Path(__file__).resolve().parent.parent / ".github/workflows/wp-refill.yml"
        self.assertTrue(workflow.is_file())
        source = workflow.read_text(encoding="utf-8")
        self.assertIn("wp_generate.py --all", source)
        self.assertIn("wp_plan.py --board", source)
        self.assertIn("wp_dashboard.py", source)

    def test_dashboard_counts_closed_wps_from_commits(self) -> None:
        self._git("init")
        self._git("config", "user.email", "test@megi.local")
        self._git("config", "user.name", "Megi Test")
        (self.root / "README.md").write_text("x\n", encoding="utf-8")
        self._git("add", ".")
        self._git("commit", "-m", "feat(wp): WP-DEPTH-m0-01 close report")
        self._git("commit", "--allow-empty", "-m", "fix(wp): WP-SITE-example close warnings")
        queue = self.root / "docs/work-packages/queue"
        queue.mkdir(parents=True)
        (queue / "WP-DEPTH-m1-01.toml").write_text(
            'id = "WP-DEPTH-m1-01"\nsource = "depth"\nfile = "docs/x.md"\n'
            'expected = "ok"\nactual = "pending"\nspec = "s"\nacceptance = "done"\n'
            'verify_cmd = "python3 tools/check_arch.py"\nmetric = "depth_closed"\n'
            'milestone = "M1"\nimpact = 3\nfrequency = 3\nprofile = "light"\n',
            encoding="utf-8",
        )
        data = wp_dashboard.collect(self.root)
        self.assertEqual(data["closed"], 2)
        self.assertEqual(data["queued"], 1)
        self.assertEqual(data["closed_ids"], ["WP-DEPTH-m0-01", "WP-SITE-example"])
        html = wp_dashboard.render(data)
        self.assertIn("WP-DEPTH-m0-01", html)
        self.assertIn(">2<", html)

    def test_no_estimated_numbers(self) -> None:
        data = {"closed": 1, "queued": 0, "closed_ids": ["WP-X-1"],
                "light": 0, "heavy": 0, "head": "abc1234"}
        html = wp_dashboard.render(data).lower()
        self.assertNotIn("estimate", html)
        self.assertNotIn("aprox", html)
        self.assertNotIn("~", html)


if __name__ == "__main__":
    unittest.main()
