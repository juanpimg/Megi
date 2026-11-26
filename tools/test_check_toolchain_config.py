#!/usr/bin/env python3
"""Unit tests for tools/check_toolchain_config.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_toolchain_config.py")
_spec = importlib.util.spec_from_file_location("check_toolchain_config", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_toolchain_config"] = checker
_spec.loader.exec_module(checker)

TOOLCHAIN = '[toolchain]\nchannel = "1.99.1"\ncomponents = ["rustfmt", "clippy"]\n'
CARGO = '[workspace.package]\nedition = "2024"\nrust-version = "1.91"\n'


class VersionTests(unittest.TestCase):
    def test_parses_two_and_three_part_versions(self) -> None:
        self.assertEqual(checker.version_tuple("1.91"), (1, 91, 0))
        self.assertEqual(checker.version_tuple("1.99.1"), (1, 99, 1))

    def test_rejects_invalid_version(self) -> None:
        with self.assertRaises(ValueError):
            checker.version_tuple("nightly")


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "rust-toolchain.toml").write_text(TOOLCHAIN, encoding="utf-8")
        (self.root / "Cargo.toml").write_text(CARGO, encoding="utf-8")

    def test_valid_configuration_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_old_channel_is_reported(self) -> None:
        (self.root / "rust-toolchain.toml").write_text(
            TOOLCHAIN.replace("1.99.1", "1.90.0"), encoding="utf-8"
        )
        self.assertTrue(any("older than MSRV" in p for p in checker.validate(self.root)))

    def test_missing_components_are_reported(self) -> None:
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.99.1"\ncomponents = []\n', encoding="utf-8"
        )
        problems = checker.validate(self.root)
        self.assertTrue(any("non-empty list" in p for p in problems))

    def test_missing_rustfmt_is_reported(self) -> None:
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.99.1"\ncomponents = ["clippy"]\n', encoding="utf-8"
        )
        self.assertTrue(any("missing rustfmt" in p for p in checker.validate(self.root)))

    def test_wrong_edition_is_reported(self) -> None:
        (self.root / "Cargo.toml").write_text(CARGO.replace("2024", "2021"), encoding="utf-8")
        self.assertTrue(any("unexpected edition" in p for p in checker.validate(self.root)))

    def test_missing_files_are_reported(self) -> None:
        (self.root / "Cargo.toml").unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_configuration(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("toolchain configuration consistent", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("TOOLCHAIN:", result.stderr)


if __name__ == "__main__":
    unittest.main()
