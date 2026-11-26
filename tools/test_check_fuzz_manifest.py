#!/usr/bin/env python3
"""Unit tests for tools/check_fuzz_manifest.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_fuzz_manifest.py")
_spec = importlib.util.spec_from_file_location("check_fuzz_manifest", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_fuzz_manifest"] = checker
_spec.loader.exec_module(checker)

MANIFEST = '[[bin]]\nname = "demo"\npath = "fuzz_targets/demo.rs"\n'


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        fuzz = self.root / "fuzz"
        (fuzz / "fuzz_targets").mkdir(parents=True)
        (fuzz / "corpus/demo").mkdir(parents=True)
        (fuzz / "Cargo.toml").write_text(MANIFEST, encoding="utf-8")
        (fuzz / "fuzz_targets/demo.rs").write_text("fn main() {}\n", encoding="utf-8")

    def test_valid_target_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_manifest_is_reported(self) -> None:
        (self.root / "fuzz/Cargo.toml").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_empty_manifest_is_reported(self) -> None:
        (self.root / "fuzz/Cargo.toml").write_text("", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["fuzz/Cargo.toml has no [[bin]] targets"])

    def test_missing_source_is_reported(self) -> None:
        (self.root / "fuzz/fuzz_targets/demo.rs").unlink()
        self.assertTrue(any("missing target source" in p for p in checker.validate(self.root)))

    def test_name_mismatch_is_reported(self) -> None:
        (self.root / "fuzz/Cargo.toml").write_text(
            MANIFEST.replace('name = "demo"', 'name = "other"'), encoding="utf-8"
        )
        self.assertTrue(any("does not match target name" in p for p in checker.validate(self.root)))

    def test_missing_corpus_is_reported(self) -> None:
        import shutil

        shutil.rmtree(self.root / "fuzz/corpus/demo")
        self.assertTrue(any("missing corpus directory" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_manifest(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fuzz manifest OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("FUZZ MANIFEST:", result.stderr)


if __name__ == "__main__":
    unittest.main()
