"""Tests for the milestone acceptance criterion checker (WP engine, spec §4.3)."""
import contextlib
import io
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

    def test_missing_progress_file_fails_without_raising(self) -> None:
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("docs/status/progress.toml not found", output.getvalue())

    def test_unknown_milestone_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = ["informe WPT"]\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M99", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("FAIL: milestone M99 not found", output.getvalue())

    def test_out_of_range_criterion_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = ["informe WPT"]\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 99, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("FAIL: criterion M0#99 does not exist", output.getvalue())

    def test_empty_string_evidence_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = [""]\npending = []\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("evidence", output.getvalue())

    def test_whitespace_only_evidence_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["   "]\npending = []\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("no recorded evidence", output.getvalue())

    def test_table_evidence_fails_without_raising(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = [{text = "x"}]\npending = []\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("no recorded evidence", output.getvalue())

    def test_empty_string_pending_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = [""]\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("no recorded evidence", output.getvalue())


if __name__ == "__main__":
    unittest.main()
