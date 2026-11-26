#!/usr/bin/env python3
"""Unit tests for tools/check_wp_generators.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wp_generators.py")
_spec = importlib.util.spec_from_file_location("check_wp_generators", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wp_generators"] = checker
_spec.loader.exec_module(checker)

SCHEMA = """## Fuentes y generadores

| Fuente | Generador | Métrica |
|---|---|---|
| Fallos WPT | `tools/wp_generate.py wpt` | wpt_passed |
| Fallos Test262 | `... js262` | js262_passed |
## Ciclo del agente
"""
CODE = 'SOURCES = ("wpt", "js262")\n'


class ParseTests(unittest.TestCase):
    def test_documented_sources_include_full_and_short_forms(self) -> None:
        self.assertEqual(checker.documented_sources(SCHEMA), {"wpt", "js262"})

    def test_implemented_sources(self) -> None:
        self.assertEqual(checker.implemented_sources(CODE), {"wpt", "js262"})

    def test_missing_sources_constant_is_empty(self) -> None:
        self.assertEqual(checker.implemented_sources("nada"), set())


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs/work-packages").mkdir(parents=True)
        (self.root / "tools").mkdir()
        (self.root / "docs/work-packages/SCHEMA.md").write_text(SCHEMA, encoding="utf-8")
        (self.root / "tools/wp_generate.py").write_text(CODE, encoding="utf-8")

    def test_matching_docs_pass(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_files_are_reported(self) -> None:
        (self.root / "tools/wp_generate.py").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_undocumented_source_is_reported(self) -> None:
        (self.root / "tools/wp_generate.py").write_text(
            'SOURCES = ("wpt", "js262", "site")\n', encoding="utf-8"
        )
        self.assertIn("site: SOURCES entry has no documented generator", checker.validate(self.root))

    def test_unimplemented_source_is_reported(self) -> None:
        (self.root / "docs/work-packages/SCHEMA.md").write_text(
            SCHEMA.replace("| Fallos Test262 | `... js262` | js262_passed |", ""), encoding="utf-8"
        )
        self.assertIn("js262: SOURCES entry has no documented generator", checker.validate(self.root))

    def test_empty_docs_are_reported(self) -> None:
        (self.root / "docs/work-packages/SCHEMA.md").write_text("nada", encoding="utf-8")
        self.assertTrue(any("no generators documented" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_generators(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("work package generators consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WP GENERATORS:", result.stderr)


if __name__ == "__main__":
    unittest.main()
