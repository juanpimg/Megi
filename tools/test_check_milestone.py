"""Tests for the milestone acceptance criterion checker (WP engine, spec §4.3)."""
import tempfile
import unittest
from pathlib import Path

from tools import check_milestone


class MilestoneCheckerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write(self, relative: str, content: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def test_milestone_checker_is_red_then_green(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = ["informe WPT"]\n')
        self.assertNotEqual(check_milestone.check("M0", 1, root=self.root), 0)
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["informe WPT"]\npending = []\n')
        self.assertEqual(check_milestone.check("M0", 1, root=self.root), 0)


if __name__ == "__main__":
    unittest.main()
