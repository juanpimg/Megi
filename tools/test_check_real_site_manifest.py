#!/usr/bin/env python3
"""Unit tests for tools/check_real_site_manifest.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_real_site_manifest.py")
_spec = importlib.util.spec_from_file_location("check_real_site_manifest", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_real_site_manifest"] = checker
_spec.loader.exec_module(checker)


def site(**overrides: object) -> dict:
    values = {
        "id": "example",
        "name": "example.com",
        "url": "https://example.com",
        "snapshot": "example.html",
        "min_items": 5,
    }
    values.update(overrides)
    return values


class ValidateTests(unittest.TestCase):
    def test_valid_site_passes(self) -> None:
        self.assertEqual(checker.validate({"site": [site()]}), [])

    def test_empty_manifest_is_reported(self) -> None:
        self.assertEqual(checker.validate({}), ["manifest has no [[site]] entries"])

    def test_duplicate_id_is_reported(self) -> None:
        problems = checker.validate({"site": [site(), site()]})
        self.assertEqual(problems, ["example: duplicate id"])

    def test_missing_id_and_name_are_reported(self) -> None:
        problems = checker.validate({"site": [site(id="", name="")]})
        self.assertIn("site[0]: missing id", problems)
        self.assertTrue(any("missing name" in p for p in problems))

    def test_bad_url_is_reported(self) -> None:
        problems = checker.validate({"site": [site(url="ftp://example.com")]})
        self.assertTrue(any("http(s) URL" in p for p in problems))

    def test_unsafe_snapshot_path_is_reported(self) -> None:
        problems = checker.validate({"site": [site(snapshot="../secret.html")]})
        self.assertTrue(any("unsafe snapshot path" in p for p in problems))

    def test_snapshot_extension_is_reported(self) -> None:
        problems = checker.validate({"site": [site(snapshot="page.txt")]})
        self.assertTrue(any("must be an .html file" in p for p in problems))

    def test_invalid_min_items_is_reported(self) -> None:
        problems = checker.validate({"site": [site(min_items=0)]})
        self.assertTrue(any("positive integer" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_manifest(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("real-site manifest OK (4 sites)", result.stdout)

    def test_cli_reports_unreadable_manifest(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--manifest", "DOES-NOT-EXIST.toml"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("check_real_site_manifest:", result.stderr)


if __name__ == "__main__":
    unittest.main()
