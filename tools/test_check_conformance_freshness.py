#!/usr/bin/env python3
"""Unit tests for tools/check_conformance_freshness.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

MODULE_PATH = Path(__file__).with_name("check_conformance_freshness.py")
_spec = importlib.util.spec_from_file_location("check_conformance_freshness", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_conformance_freshness"] = checker
_spec.loader.exec_module(checker)


class GatePathsTests(unittest.TestCase):
    def test_update_coverage_shares_the_same_gate_paths(self) -> None:
        # #2444: a render change must invalidate both checks; importing the
        # tuple here fails if update_coverage drifts from this list again.
        from tools import update_coverage

        self.assertEqual(
            update_coverage.GATE_RELEVANT_PATHS,
            tuple(f"{path}/" for path in checker.GATE_PATHS),
        )
        self.assertIn("crates/render/", update_coverage.GATE_RELEVANT_PATHS)


class AnalyzeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "docs/status").mkdir(parents=True)
        (self.root / "docs/status/progress.toml").write_text(
            '[gate]\nverified_commit = "abc123"\n', encoding="utf-8"
        )

    @staticmethod
    def _fake(changed: list[str], ancestor: int = 0):
        def run(args, **kwargs):
            if "merge-base" in args:
                return SimpleNamespace(returncode=ancestor, stdout="", stderr="")
            if "diff" in args:
                return SimpleNamespace(returncode=0, stdout="\n".join(changed), stderr="")
            raise AssertionError(args)
        return run

    def test_fresh_stamp(self) -> None:
        info = checker.analyze(self.root, self._fake([]))
        self.assertFalse(info["stale"])
        self.assertEqual(info["commit"], "abc123")

    def test_stale_stamp_lists_paths(self) -> None:
        info = checker.analyze(self.root, self._fake(["crates/css/src/a.rs", "crates/html/src/b.rs"]))
        self.assertTrue(info["stale"])
        self.assertEqual(len(info["changed_paths"]), 2)

    def test_non_ancestor_commit_is_stale(self) -> None:
        info = checker.analyze(self.root, self._fake([], ancestor=1))
        self.assertTrue(info["stale"])
        self.assertIn("not an ancestor", info["reason"])

    def test_missing_manifest_is_reported(self) -> None:
        (self.root / "docs/status/progress.toml").unlink()
        info = checker.analyze(self.root, self._fake([]))
        self.assertIn("error", info)

    def test_missing_commit_is_reported(self) -> None:
        (self.root / "docs/status/progress.toml").write_text("[gate]\n", encoding="utf-8")
        info = checker.analyze(self.root, self._fake([]))
        self.assertIn("error", info)


class CliTests(unittest.TestCase):
    def test_informational_cli_passes_on_stale_repo(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CONFORMANCE FRESHNESS:", result.stdout)

    def test_strict_cli_fails_on_stale_repo(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs/status").mkdir(parents=True)
            (root / "docs/status/progress.toml").write_text(
                '[gate]\nverified_commit = "abc123"\n', encoding="utf-8"
            )
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", str(root), "--strict"],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("STALE", result.stdout)


if __name__ == "__main__":
    unittest.main()
