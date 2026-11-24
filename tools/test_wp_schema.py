"""Tests for the work-package schema (WP engine, spec §4.3)."""
import unittest
from tools import wp_schema

class WorkPackageSchemaTests(unittest.TestCase):
    def _valid(self):
        return {"id": "WP-WPT-abc123", "source": "wpt", "file": "crates/css/src/lib.rs",
                "expected": "subtest PASS", "actual": "subtest FAIL", "spec": "CSS Syntax L3 §4",
                "acceptance": "subtest passes and regression test added",
                "verify_cmd": "cargo test -p megi-css wpt_regression",
                "metric": "wpt_passed", "milestone": "M0", "impact": 5, "frequency": 3}

    def test_valid_work_package_is_accepted(self):
        wp = wp_schema.validate_wp(self._valid())
        self.assertEqual(wp.id, "WP-WPT-abc123")
        self.assertEqual(wp.metric, "wpt_passed")

    def test_missing_verify_cmd_is_rejected(self):
        data = self._valid(); del data["verify_cmd"]
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("verify_cmd", str(ctx.exception))

    def test_metric_is_required(self):
        data = self._valid(); del data["metric"]
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("metric", str(ctx.exception))

    def test_id_must_match_prefix_convention(self):
        data = self._valid(); data["id"] = "random-id"
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("id", str(ctx.exception))

    def test_acceptance_must_be_nonempty(self):
        data = self._valid(); data["acceptance"] = ""
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("acceptance", str(ctx.exception))

    def test_metric_must_be_one_of_the_seven_declared(self):
        data = self._valid(); data["metric"] = "made_up_metric"
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("metric", str(ctx.exception))

    def test_missing_profile_is_derived_from_verify_cmd(self):
        wp = wp_schema.validate_wp(self._valid())
        self.assertEqual(wp.profile, "heavy")

    def test_python_verify_cmd_derives_light_profile(self):
        data = self._valid(); data["verify_cmd"] = "python3 tools/check_docs_links.py"
        self.assertEqual(wp_schema.validate_wp(data).profile, "light")

    def test_empty_profile_is_derived_from_verify_cmd(self):
        data = self._valid(); data["profile"] = ""
        self.assertEqual(wp_schema.validate_wp(data).profile, "heavy")

    def test_uppercase_profile_is_normalized(self):
        data = self._valid(); data["profile"] = "LIGHT"
        self.assertEqual(wp_schema.validate_wp(data).profile, "light")

    def test_unknown_profile_is_rejected(self):
        data = self._valid(); data["profile"] = "typo"
        with self.assertRaises(ValueError) as ctx: wp_schema.validate_wp(data)
        self.assertIn("profile", str(ctx.exception))

if __name__ == "__main__": unittest.main()
