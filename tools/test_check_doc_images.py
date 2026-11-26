#!/usr/bin/env python3
"""Unit tests for tools/check_doc_images.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_doc_images.py")
_spec = importlib.util.spec_from_file_location("check_doc_images", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_doc_images"] = checker
_spec.loader.exec_module(checker)


class ExtractTests(unittest.TestCase):
    def test_finds_both_path_styles(self) -> None:
        text = "![a](docs/images/a.png) y ![b](images/b.jpg)"
        self.assertEqual(checker.extract_refs(text), {"docs/images/a.png", "images/b.jpg"})

    def test_ignores_unrelated_paths(self) -> None:
        self.assertEqual(checker.extract_refs("ver docs/status/coverage.svg"), set())


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        docs = self.root / "docs"
        docs.mkdir()
        (docs / "GUIDE.md").write_text("![a](docs/images/a.png)\n", encoding="utf-8")
        (self.root / "docs/images").mkdir()
        (self.root / "docs/images/a.png").write_bytes(b"png")

    def test_valid_reference_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_relative_reference_resolves_against_doc(self) -> None:
        (self.root / "docs/GUIDE.md").write_text("![b](images/b.png)\n", encoding="utf-8")
        (self.root / "docs/images/b.png").write_bytes(b"png")
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_image_is_reported(self) -> None:
        (self.root / "docs/GUIDE.md").write_text("![x](docs/images/x.png)\n", encoding="utf-8")
        self.assertEqual(
            checker.validate(self.root), ["docs/images/x.png: referenced image is missing"]
        )

    def test_missing_docs_dir_is_reported(self) -> None:
        import shutil

        shutil.rmtree(self.root / "docs")
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_no_references_is_reported(self) -> None:
        (self.root / "docs/GUIDE.md").write_text("sin imágenes\n", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["no documentation images referenced"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_docs(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("documentation images OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("DOC IMAGE:", result.stderr)


if __name__ == "__main__":
    unittest.main()
