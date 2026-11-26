"""Tests for the milestone acceptance criterion checker (WP engine, spec §4.3)."""
import contextlib
import io
import tempfile
import tomllib
import unittest
from pathlib import Path

from tools import check_milestone, progress_stats


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
                    '[[phase]]\nid = "M0"\ntitle = "x"\n'
                    'items = ["informe WPT — `docs/status/wpt.md` PASS"]\npending = []\n')
        self.assertEqual(check_milestone.check("M0", 1, root=self.root), 0)

    def test_documented_string_is_not_a_verified_criterion(self) -> None:
        # #2441: moving bare text into `items` must not turn it into a PASS.
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["informe WPT"]\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("documented without evidence", output.getvalue())

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

    def test_integer_phase_id_does_not_crash(self) -> None:
        """A numeric TOML id must be stringified, not crash .upper() lookup."""
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = 0\ntitle = "x"\n'
                    'items = ["evidence — `python3 tools/check_milestone.py` PASS"]\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("0", 1, root=self.root)
        self.assertEqual(result, 0)
        self.assertIn("verified", output.getvalue())
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("milestone M0 not found", output.getvalue())

    def test_string_items_is_rejected_not_false_pass(self) -> None:
        """A string `items` must not let items[index-1] yield a character as PASS."""
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = "verified criterion"\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("malformed criterion list", output.getvalue())
        self.assertNotIn("verified: v", output.getvalue())

    def test_string_pending_is_rejected(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = "still pending"\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("malformed criterion list", output.getvalue())

    def test_match_mode_rejects_malformed_lists(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = "verified criterion"\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", None, root=self.root, match="verified")
        self.assertNotEqual(result, 0)
        self.assertIn("malformed criterion list", output.getvalue())

    def test_empty_string_pending_fails(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = [""]\n')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = check_milestone.check("M0", 1, root=self.root)
        self.assertNotEqual(result, 0)
        self.assertIn("no recorded evidence", output.getvalue())

    def test_match_mode_is_red_then_green(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = ["informe WPT"]\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", None, root=self.root, match="informe WPT")
        self.assertNotEqual(result, 0)
        self.assertIn("still pending", output.getvalue())
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\n'
                    'items = ["informe WPT — `docs/status/wpt.md` PASS"]\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", None, root=self.root, match="informe WPT")
        self.assertEqual(result, 0)
        self.assertIn("verified", output.getvalue())

    def test_match_mode_is_substring_and_case_insensitive(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\n'
                    'items = ["Release con verify + firma — `cosign verify` PASS"]\npending = []\n')
        self.assertEqual(
            check_milestone.check("M0", None, root=self.root, match="COSIGN"), 0
        )

    def test_match_mode_documented_string_is_not_verified(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["informe WPT"]\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check("M0", None, root=self.root, match="informe WPT")
        self.assertNotEqual(result, 0)
        self.assertIn("documented without evidence", output.getvalue())

    def test_match_mode_unknown_and_empty_text_fail(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["informe WPT"]\npending = []\n')
        for needle, expected in (("no existe", "does not match"), ("   ", "--match")):
            with contextlib.redirect_stdout(io.StringIO()) as output:
                result = check_milestone.check("M0", None, root=self.root, match=needle)
            self.assertNotEqual(result, 0)
            self.assertIn(expected, output.getvalue())

    def test_match_mode_real_release_criterion_is_stable(self) -> None:
        repo_root = Path(__file__).resolve().parent.parent
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check(
                "M9", None, root=repo_root, match="Distribución multiplataforma firmada"
            )
        self.assertEqual(result, 0)
        self.assertIn("verified", output.getvalue())
        with contextlib.redirect_stdout(io.StringIO()) as output:
            result = check_milestone.check(
                "M9", None, root=repo_root, match="Auditoría externa y bus factor"
            )
        self.assertNotEqual(result, 0)
        self.assertIn("still pending", output.getvalue())

    def test_cli_match_flag_and_positional_mode(self) -> None:
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\n'
                    'items = ["informe WPT — `docs/status/wpt.md` PASS"]\npending = []\n')
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(
                check_milestone.main(
                    ["M0", "--match", "informe WPT", "--root", str(self.root)]
                ),
                0,
            )
            self.assertEqual(check_milestone.main(["M0", "1", "--root", str(self.root)]), 0)
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as raised:
                check_milestone.main(["M0", "--root", str(self.root)])
            self.assertEqual(raised.exception.code, 2)


class ProgressEvidenceCountTests(unittest.TestCase):
    def test_documented_text_is_not_counted_or_closing(self) -> None:
        phase = {"id": "M0", "items": ["informe WPT"], "pending": []}
        self.assertFalse(progress_stats.has_evidence("informe WPT"))
        self.assertEqual(progress_stats.criterion_counts(phase), (0, 1))
        self.assertEqual(progress_stats.criteria_percent(phase), 0)
        self.assertEqual(progress_stats.overall_percent([phase]), 0)
        self.assertEqual(progress_stats.milestone_counts([phase]), (0, 1))

    def test_cited_criterion_counts_and_closes_the_milestone(self) -> None:
        phase = {"id": "M0", "items": ["informe — `docs/status/wpt.md` PASS"], "pending": []}
        self.assertTrue(progress_stats.has_evidence(phase["items"][0]))
        self.assertEqual(progress_stats.criterion_counts(phase), (1, 1))
        self.assertEqual(progress_stats.criteria_percent(phase), 100)
        self.assertEqual(progress_stats.overall_counts([phase]), (1, 1))
        self.assertEqual(progress_stats.milestone_counts([phase]), (1, 1))

    def test_documented_items_stay_in_the_denominator(self) -> None:
        phase = {
            "id": "M0",
            "items": ["bare prose", "check — `python3 tools/gate.py` EXIT=0"],
            "pending": ["not done"],
        }
        self.assertEqual(progress_stats.criterion_counts(phase), (1, 3))
        self.assertEqual(progress_stats.criteria_percent(phase), 33)

    def test_malformed_lists_do_not_crash_or_false_pass(self) -> None:
        phase = {"id": "M0", "items": "not a list", "pending": None}
        self.assertEqual(progress_stats.criterion_counts(phase), (0, 0))
        self.assertEqual(progress_stats.criteria_percent(phase), 0)
        self.assertEqual(progress_stats.milestone_counts([phase]), (0, 1))

    def test_published_progress_numbers_are_unchanged(self) -> None:
        # The manifest's existing 47 criteria all carry citations or gate results;
        # stricter counting must not make the generated PROGRESS.md stale.
        root = Path(__file__).resolve().parent.parent
        with (root / "docs/status/progress.toml").open("rb") as handle:
            phases = tomllib.load(handle).get("phase", [])
        self.assertEqual(progress_stats.overall_counts(phases), (47, 64))
        self.assertEqual(progress_stats.milestone_counts(phases), (6, 13))


if __name__ == "__main__":
    unittest.main()
