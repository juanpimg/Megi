#!/usr/bin/env python3
"""Unit tests for tools/check_workflow_scripts.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_workflow_scripts.py")
_spec = importlib.util.spec_from_file_location("check_workflow_scripts", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_workflow_scripts"] = checker
_spec.loader.exec_module(checker)

HEADER = "path\tlines\tcategory\tphase\tauditor\tdate\tstatus\tfindings\tnotes\n"


class ExtractTests(unittest.TestCase):
    def test_finds_py_and_sh_paths(self) -> None:
        text = "run: python3 tools/check_arch.py && bash tools/bench.sh"
        self.assertEqual(
            checker.extract_refs(text), {"tools/check_arch.py", "tools/bench.sh"}
        )


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / ".github/workflows").mkdir(parents=True)
        (self.root / "tools").mkdir()
        (self.root / "docs/audit").mkdir(parents=True)
        (self.root / "tools/check_arch.py").write_text("", encoding="utf-8")
        (self.root / ".github/workflows/ci.yml").write_text(
            "run: python3 tools/check_arch.py\n", encoding="utf-8"
        )
        (self.root / "docs/audit/coverage.tsv").write_text(
            HEADER + "tools/check_arch.py\t1\ttool\tF3\tt\t2026-11-26\tcovered\t#t\tn\n",
            encoding="utf-8",
        )

    def test_valid_workflow_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_workflows_dir_is_reported(self) -> None:
        (self.root / ".github/workflows/ci.yml").unlink()
        (self.root / ".github/workflows").rmdir()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_no_references_is_reported(self) -> None:
        (self.root / ".github/workflows/ci.yml").write_text("name: ci\n", encoding="utf-8")
        self.assertEqual(checker.validate(self.root), ["no tools/* scripts referenced by workflows"])

    def test_missing_script_is_reported(self) -> None:
        (self.root / ".github/workflows/ci.yml").write_text(
            "run: python3 tools/absent.py\n", encoding="utf-8"
        )
        self.assertIn("tools/absent.py: referenced by a workflow but missing", checker.validate(self.root))

    def test_missing_ledger_row_is_reported(self) -> None:
        (self.root / "docs/audit/coverage.tsv").write_text(HEADER, encoding="utf-8")
        self.assertIn("tools/check_arch.py: missing from audit ledger", checker.validate(self.root))

    def test_uncovered_ledger_row_is_reported(self) -> None:
        (self.root / "docs/audit/coverage.tsv").write_text(
            HEADER + "tools/check_arch.py\t1\ttool\tF3\tt\t2026-11-26\tpending\t#t\tn\n",
            encoding="utf-8",
        )
        self.assertTrue(any("ledger status is 'pending'" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_workflows(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("workflow script references OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WORKFLOW SCRIPT:", result.stderr)


if __name__ == "__main__":
    unittest.main()
