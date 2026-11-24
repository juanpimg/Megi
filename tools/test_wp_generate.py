"""Tests for the seven WP generators (spec §4.3), including machine profile."""
import json
import tempfile
import tomllib
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
