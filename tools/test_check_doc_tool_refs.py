#!/usr/bin/env python3
"""Unit tests for tools/check_doc_tool_refs.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_doc_tool_refs.py")
_spec = importlib.util.spec_from_file_location("check_doc_tool_refs", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_doc_tool_refs"] = checker
_spec.loader.exec_module(checker)

HEADER = "path\tlines\tcategory\tphase\tauditor\tdate\tstatus\tfindings\tnotes\n"


class ParsingTests(unittest.TestCase):
    def test_extracts_only_tool_paths(self) -> None:
        text = "run `python3 tools/check_arch.py` and tools/visual_parity.py --json"
        self.assertEqual(
            checker.extract_refs(text), {"tools/check_arch.py", "tools/visual_parity.py"}
        )


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "tools").mkdir()
        (self.root / "docs").mkdir()
        (self.root / "tools/check_demo.py").write_text("print(1)\n", encoding="utf-8")
        (self.root / "tools/test_check_demo.py").write_text("x = 1\n", encoding="utf-8")
        (self.root / "docs/GUIDE.md").write_text("use tools/check_demo.py\n", encoding="utf-8")
        self.ledger = self.root / "coverage.tsv"

    def _ledger(self, path: str, lines: int, status: str = "covered") -> None:
        self.ledger.write_text(
            HEADER + f"{path}\t{lines}\tdocs\tF2\ttester\t2026-11-26\t{status}\t#t\tn\n",
            encoding="utf-8",
        )

    def test_valid_reference_passes(self) -> None:
        self._ledger("tools/check_demo.py", 1)
        self.assertEqual(checker.validate(self.root, ("docs/GUIDE.md",), self.ledger), [])

    def test_missing_tool_is_reported(self) -> None:
        self._ledger("tools/check_demo.py", 1)
        (self.root / "docs/GUIDE.md").write_text("use tools/missing.py\n", encoding="utf-8")
        problems = checker.validate(self.root, ("docs/GUIDE.md",), self.ledger)
        self.assertIn("docs/GUIDE.md: tools/missing.py does not exist", problems)

    def test_missing_test_module_is_reported(self) -> None:
        self._ledger("tools/check_demo.py", 1)
        (self.root / "tools/test_check_demo.py").unlink()
        problems = checker.validate(self.root, ("docs/GUIDE.md",), self.ledger)
        self.assertIn("docs/GUIDE.md: tools/check_demo.py has no tools/test_check_demo.py", problems)

    def test_missing_ledger_row_is_reported(self) -> None:
        self.ledger.write_text(HEADER, encoding="utf-8")
        problems = checker.validate(self.root, ("docs/GUIDE.md",), self.ledger)
        self.assertIn("docs/GUIDE.md: tools/check_demo.py missing from ledger", problems)

    def test_wrong_status_and_stale_count_are_reported(self) -> None:
        self._ledger("tools/check_demo.py", 9, status="pending")
        problems = checker.validate(self.root, ("docs/GUIDE.md",), self.ledger)
        self.assertTrue(any("is 'pending' in ledger" in p for p in problems))
        self.assertTrue(any("stale line count" in p for p in problems))

    def test_doc_without_references_is_reported(self) -> None:
        self._ledger("tools/check_demo.py", 1)
        (self.root / "docs/GUIDE.md").write_text("no tools here\n", encoding="utf-8")
        problems = checker.validate(self.root, ("docs/GUIDE.md",), self.ledger)
        self.assertIn("docs/GUIDE.md: no tool references found", problems)


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_docs(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("documented tools OK (4 docs)", result.stdout)

    def test_cli_reports_unreadable_doc(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--docs", "DOES-NOT-EXIST.md"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("TOOL REF:", result.stderr)


if __name__ == "__main__":
    unittest.main()
