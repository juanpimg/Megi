#!/usr/bin/env python3
"""Unit tests for tools/check_wp_schema_docs.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wp_schema_docs.py")
_spec = importlib.util.spec_from_file_location("check_wp_schema_docs", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wp_schema_docs"] = checker
_spec.loader.exec_module(checker)

DOCS = "## Campos obligatorios\n| Campo | Tipo | Regla |\n|---|---|---|\n| `id` | str | x |\n| `metric` | str | y |\n## Fuentes\n"
CODE = 'REQUIRED_FIELDS = ("id", "metric")\n'


class ParseTests(unittest.TestCase):
    def test_documented_fields_are_scoped_to_the_required_table(self) -> None:
        self.assertEqual(checker.documented_fields(DOCS), {"id", "metric"})

    def test_implemented_fields_come_from_required_fields(self) -> None:
        self.assertEqual(checker.implemented_fields(CODE), {"id", "metric"})

    def test_missing_required_tuple_is_empty(self) -> None:
        self.assertEqual(checker.implemented_fields("sin constantes"), set())


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs/work-packages").mkdir(parents=True)
        (self.root / "tools").mkdir()
        (self.root / "docs/work-packages/SCHEMA.md").write_text(DOCS, encoding="utf-8")
        (self.root / "tools/wp_schema.py").write_text(CODE, encoding="utf-8")

    def test_matching_docs_and_validator_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "tools/wp_schema.py").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_documented_only_field_is_reported(self) -> None:
        (self.root / "docs/work-packages/SCHEMA.md").write_text(
            DOCS.replace("| `metric` | str | y |", "| `metric` | str | y |\n| `profile` | str | z |"),
            encoding="utf-8",
        )
        self.assertIn("profile: documented but not enforced by wp_schema.py", checker.validate(self.root))

    def test_enforced_only_field_is_reported(self) -> None:
        (self.root / "tools/wp_schema.py").write_text(
            CODE.replace('"metric"', '"metric", "source"'), encoding="utf-8"
        )
        self.assertIn("source: enforced by wp_schema.py but undocumented", checker.validate(self.root))

    def test_empty_docs_are_reported(self) -> None:
        (self.root / "docs/work-packages/SCHEMA.md").write_text("nada", encoding="utf-8")
        self.assertIn("SCHEMA.md: no required fields found", checker.validate(self.root))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_schema(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("work package schema docs consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WP SCHEMA:", result.stderr)


if __name__ == "__main__":
    unittest.main()
