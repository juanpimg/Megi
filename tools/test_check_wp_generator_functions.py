#!/usr/bin/env python3
"""Unit tests for tools/check_wp_generator_functions.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wp_generator_functions.py")
_spec = importlib.util.spec_from_file_location("check_wp_generator_functions", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wp_generator_functions"] = checker
_spec.loader.exec_module(checker)

CODE = 'SOURCES = ("wpt", "site")\nGENERATORS = {\n    "wpt": generate_wpt, "site": generate_site,\n}\n'


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "tools").mkdir()
        (self.root / "tools/wp_generate.py").write_text(CODE, encoding="utf-8")

    def test_matching_maps_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_file_is_reported(self) -> None:
        (self.root / "tools/wp_generate.py").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_source_without_generator_is_reported(self) -> None:
        (self.root / "tools/wp_generate.py").write_text(
            CODE.replace('"site": generate_site,', ""), encoding="utf-8"
        )
        self.assertIn("site: SOURCES entry has no GENERATORS function", checker.validate(self.root))

    def test_generator_without_source_is_reported(self) -> None:
        (self.root / "tools/wp_generate.py").write_text(
            CODE.replace('SOURCES = ("wpt", "site")', 'SOURCES = ("wpt",)'), encoding="utf-8"
        )
        self.assertIn("site: GENERATORS function has no SOURCES entry", checker.validate(self.root))

    def test_missing_constants_are_reported(self) -> None:
        (self.root / "tools/wp_generate.py").write_text("nada\n", encoding="utf-8")
        problems = checker.validate(self.root)
        self.assertIn("wp_generate.py: SOURCES not found", problems)
        self.assertIn("wp_generate.py: GENERATORS not found", problems)


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_generator(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("work package generator functions consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WP GENERATOR FUNCTIONS:", result.stderr)


if __name__ == "__main__":
    unittest.main()
