"""Tests for the seven WP generators (spec §4.3), including machine profile."""
import contextlib
import io
import json
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from tools import wp_generate
from tools.wp_schema import validate_wp


class GeneratorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write(self, relative: str, content: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    def _generate(self, source: str, dry_run: bool = True):
        return wp_generate.generate(source, dry_run=dry_run, root=self.root)

    def test_sources_are_registered(self) -> None:
        self.assertEqual(
            set(wp_generate.SOURCES),
            {"wpt", "js262", "site", "fuzz", "audit", "depth", "user"},
        )

    def test_wpt_failure_becomes_valid_wp(self) -> None:
        self._write(
            "tests/wpt/results/latest.json",
            json.dumps({"failures": [
                {"test": "html/syntax.html", "subtest": "comment",
                 "expected": "PASS", "actual": "FAIL", "crate": "html", "milestone": "M0"},
            ]}),
        )
        wps = self._generate("wpt")
        self.assertEqual(len(wps), 1)
        self.assertTrue(wps[0].id.startswith("WP-WPT-"))
        self.assertEqual(wps[0].metric, "wpt_passed")
        validate_wp(vars(wps[0]))

    def test_js262_failure_becomes_valid_wp(self) -> None:
        self._write(
            "tests/test262/results/latest.json",
            json.dumps({"failures": [{"test": "built-ins/Array/foo.js",
                                      "expected": "PASS", "actual": "FAIL", "milestone": "M8"}]}),
        )
        wps = self._generate("js262")
        self.assertEqual(len(wps), 1)
        self.assertEqual(wps[0].metric, "js262_passed")
        validate_wp(vars(wps[0]))

    def test_site_generator_skips_passing_sites(self) -> None:
        self._write("docs/status/real-sites.json", json.dumps({"results": [
            {"id": "example", "name": "example.com", "result": "DEGRADADO",
             "warnings": ["console.error: TypeError: not a callable function"], "issue": 0},
            {"id": "hn", "name": "news.ycombinator.com", "result": "PASS", "warnings": []},
        ]}))
        wps = self._generate("site")
        self.assertEqual([wp.id for wp in wps], ["WP-SITE-example"])
        self.assertEqual(wps[0].metric, "sites_passed")
        self.assertIn("TypeError", wps[0].actual)

    def test_fuzz_crash_becomes_valid_wp(self) -> None:
        self._write("fuzz/artifacts/crash.json", json.dumps({"crashes": [
            {"target": "html_parse", "id": "abc123", "note": "stack overflow"},
        ]}))
        wps = self._generate("fuzz")
        self.assertEqual(len(wps), 1)
        self.assertEqual(wps[0].metric, "fuzz_crashes")
        validate_wp(vars(wps[0]))

    def test_audit_pending_row_becomes_valid_wp(self) -> None:
        self._write(
            "docs/audit/coverage.tsv",
            "path\tlines\tcategory\tphase\tauditor\tdate\tstatus\tfindings\tnotes\n"
            "docs/new.md\t10\tdocs\tF3\taudit\t2026-11-24\tpending\t-\tnot read\n"
            "docs/ok.md\t5\tdocs\tF3\taudit\t2026-11-24\tcovered\t-\tread\n",
        )
        wps = self._generate("audit")
        self.assertEqual(len(wps), 1)
        self.assertEqual(wps[0].metric, "audit_pending")
        self.assertIn("docs/new.md", wps[0].file)

    def test_depth_inventory_becomes_valid_wp(self) -> None:
        self._write(
            "docs/work-packages/depth-inventory.toml",
            '[[entry]]\nid = "M0-WPT-REPORT"\ngap = "informe WPT con P/T y SHA"\n'
            'spec = "spec v2 §4.3"\nverify_cmd = "python3 tools/check_arch.py"\n'
            'metric = "depth_closed"\nmilestone = "M0"\nimpact = 5\nfrequency = 3\n',
        )
        wps = self._generate("depth")
        self.assertEqual(len(wps), 1)
        self.assertEqual(wps[0].metric, "depth_closed")
        self.assertEqual(wps[0].priority, 15)

    def test_user_report_becomes_valid_wp(self) -> None:
        self._write("docs/work-packages/user-reports.json", json.dumps([
            {"number": 17, "title": "Broken site: example.com",
             "body": "blank page", "labels": ["broken_site"]},
        ]))
        wps = self._generate("user")
        self.assertEqual([wp.id for wp in wps], ["WP-USER-17"])
        self.assertEqual(wps[0].metric, "user_issues")

    def test_dry_run_writes_nothing_and_write_mode_persists_valid_toml(self) -> None:
        self._write("docs/status/real-sites.json", json.dumps({"results": [
            {"id": "example", "name": "example.com", "result": "DEGRADADO",
             "warnings": ["console error"], "issue": 0},
        ]}))
        self._generate("site", dry_run=True)
        self.assertFalse((self.root / "docs/work-packages/queue").exists())
        self._generate("site", dry_run=False)
        files = list((self.root / "docs/work-packages/queue").glob("WP-SITE-*.toml"))
        self.assertEqual(len(files), 1)
        with files[0].open("rb") as handle:
            data = tomllib.load(handle)
        validate_wp(data)

    def test_multiline_values_round_trip_as_valid_toml(self) -> None:
        self._write("docs/status/real-sites.json", json.dumps({"results": [
            {"id": "wikipedia", "name": "wikipedia", "result": "DEGRADADO",
             "warnings": ["console.error: TypeError: x\n    at <main> (unknown:1)"], "issue": 0},
        ]}))
        self._generate("site", dry_run=False)
        path = self.root / "docs/work-packages/queue/WP-SITE-wikipedia.toml"
        with path.open("rb") as handle:
            data = tomllib.load(handle)  # fails today: raw newline inside a TOML string
        self.assertIn("at <main>", validate_wp(data).actual)

    def test_duplicate_ids_are_not_written_twice(self) -> None:
        self._write("docs/work-packages/user-reports.json", json.dumps([
            {"number": 17, "title": "Broken site", "body": "x", "labels": ["broken_site"]},
        ]))
        self._generate("user", dry_run=False)
        self._generate("user", dry_run=False)
        files = list((self.root / "docs/work-packages/queue").glob("WP-USER-17.toml"))
        self.assertEqual(len(files), 1)

    def test_profile_is_light_for_python_and_heavy_for_cargo(self) -> None:
        self._write(
            "docs/work-packages/depth-inventory.toml",
            '[[entry]]\nid = "docs-only"\ngap = "g"\nspec = "s"\n'
            'verify_cmd = "python3 tools/update_coverage.py --check"\n'
            'metric = "depth_closed"\nmilestone = "M0"\n'
            '[[entry]]\nid = "compile"\ngap = "g"\nspec = "s"\n'
            'verify_cmd = "cargo test -p megi-html"\n'
            'metric = "depth_closed"\nmilestone = "M0"\n',
        )
        wps = {wp.id: wp for wp in self._generate("depth")}
        self.assertEqual(wps["WP-DEPTH-docs-only"].profile, "light")
        self.assertEqual(wps["WP-DEPTH-compile"].profile, "heavy")


    def test_user_labels_as_github_objects(self) -> None:
        self._write("docs/work-packages/user-reports.json", json.dumps([
            {"number": 18, "title": "Crash", "body": "x", "labels": [{"name": "crash"}]},
        ]))
        self.assertEqual([wp.id for wp in self._generate("user")], ["WP-USER-18"])

    def test_one_bad_artifact_does_not_stop_other_sources(self) -> None:
        self._write("fuzz/artifacts/broken.json", "{not json")
        self._write("docs/status/real-sites.json", json.dumps({"results": [
            {"id": "example", "result": "DEGRADADO", "warnings": ["error"]},
        ]}))
        with contextlib.redirect_stdout(io.StringIO()):
            wps, errors = wp_generate.generate_all(self.root, dry_run=True)
        self.assertTrue(any(wp.id.startswith("WP-SITE-") for wp in wps))
        self.assertTrue(errors)
        self.assertIn("fuzz", errors[0])

    def test_numeric_fields_are_serialised_as_numbers(self) -> None:
        self._write("docs/work-packages/depth-inventory.toml",
                    '[[entry]]\nid = "x"\ngap = "g"\nspec = "s"\n'
                    'verify_cmd = "python3 tools/check_arch.py"\n'
                    'metric = "depth_closed"\nmilestone = "M0"\nimpact = 3\nfrequency = 4\n')
        self._generate("depth", dry_run=False)
        path = self.root / "docs/work-packages/queue/WP-DEPTH-x.toml"
        with path.open("rb") as handle:
            data = tomllib.load(handle)
        self.assertEqual(data["impact"], 3)
        self.assertEqual(data["frequency"], 4)

    def test_milestone_checker_is_red_then_green(self) -> None:
        from tools import check_milestone
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = []\npending = ["informe WPT"]\n')
        self.assertNotEqual(check_milestone.check("M0", 1, root=self.root), 0)
        self._write("docs/status/progress.toml",
                    '[[phase]]\nid = "M0"\ntitle = "x"\nitems = ["informe WPT"]\npending = []\n')
        self.assertEqual(check_milestone.check("M0", 1, root=self.root), 0)

    def test_js262_id_uses_the_spec_prefix(self) -> None:
        self._write("tests/test262/results/latest.json", json.dumps({"failures": [
            {"test": "built-ins/Array/foo.js", "expected": "PASS", "actual": "FAIL"},
        ]}))
        self.assertEqual([wp.id for wp in self._generate("js262")],
                         ["WP-JS-built-ins-array-foo-js"])

    def test_invalid_json_and_wrong_containers_are_ignored(self) -> None:
        self._write("tests/wpt/results/latest.json", "{not json")
        self._write("tests/test262/results/latest.json", json.dumps({"failures": "oops"}))
        self._write("docs/status/real-sites.json", json.dumps({"results": {"id": "x"}}))
        self._write("fuzz/artifacts/broken.json", json.dumps({"crashes": [42, None, "x"]}))
        for source in ("wpt", "js262", "site", "fuzz"):
            with self.subTest(source=source):
                self.assertEqual(self._generate(source), [])

    def test_non_dict_entries_are_discarded(self) -> None:
        self._write("tests/wpt/results/latest.json", json.dumps({"failures": [
            "bad", 7, {"test": "html/ok.html"},
        ]}))
        self._write("docs/status/real-sites.json", json.dumps({"results": [
            "bad", {"id": "ok-site", "result": "DEGRADADO"},
        ]}))
        self._write("fuzz/artifacts/crash.json", json.dumps({"crashes": [
            "bad", {"target": "parse", "id": "1"},
        ]}))
        self.assertEqual([wp.id for wp in self._generate("wpt")], ["WP-WPT-html-ok-html"])
        self.assertEqual([wp.id for wp in self._generate("site")], ["WP-SITE-ok-site"])
        self.assertEqual([wp.id for wp in self._generate("fuzz")], ["WP-FUZZ-parse-1"])

    def test_bad_depth_inventory_is_ignored(self) -> None:
        self._write("docs/work-packages/depth-inventory.toml", "[[entry]\nid = 1\n")
        self.assertEqual(self._generate("depth"), [])

    def test_non_numeric_depth_fields_are_ignored(self) -> None:
        self._write(
            "docs/work-packages/depth-inventory.toml",
            '[[entry]]\nid = "x"\ngap = "g"\nspec = "s"\n'
            'verify_cmd = "python3 tools/check_arch.py"\n'
            'impact = "many"\nfrequency = "often"\n',
        )
        self.assertEqual(self._generate("depth"), [])

    def test_depth_wrong_entry_type_is_ignored(self) -> None:
        self._write("docs/work-packages/depth-inventory.toml", 'entry = "bad"\n')
        self.assertEqual(self._generate("depth"), [])

    def test_github_hyphen_label_is_accepted(self) -> None:
        self._write("docs/work-packages/user-reports.json", json.dumps([
            {"number": 19, "title": "Broken site", "body": "x",
             "labels": [{"name": "broken-site"}]},
        ]))
        self.assertEqual([wp.id for wp in self._generate("user")], ["WP-USER-19"])

    def test_gh_garbage_is_ignored(self) -> None:
        with mock.patch("tools.wp_generate.subprocess.run") as run:
            run.return_value = mock.Mock(returncode=0, stdout='{"oops": true}')
            self.assertEqual(self._generate("user"), [])

    def test_generate_all_deduplicates_colliding_ids(self) -> None:
        self._write("tests/wpt/results/latest.json", json.dumps({"failures": [
            {"test": "html/a.html", "subtest": "x"},
            {"test": "html/a.html", "subtest": "x"},
            {"test": "html/b.html", "subtest": "y"},
        ]}))
        with contextlib.redirect_stdout(io.StringIO()):
            wps, errors = wp_generate.generate_all(self.root, dry_run=True)
        self.assertEqual(errors, [])
        self.assertEqual([wp.id for wp in wps],
                         ["WP-WPT-html-a-html-x", "WP-WPT-html-b-html-y"])

    def test_unknown_source_still_raises_value_error(self) -> None:
        with self.assertRaises(ValueError):
            self._generate("nope")


if __name__ == "__main__":
    unittest.main()
