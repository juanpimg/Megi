#!/usr/bin/env python3
"""Unit tests for tools/check_parity_ledger.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_parity_ledger.py")
_spec = importlib.util.spec_from_file_location("check_parity_ledger", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_parity_ledger"] = checker
_spec.loader.exec_module(checker)

HEADER = "path\tlines\tcategory\tphase\tauditor\tdate\tstatus\tfindings\tnotes\n"


def row(path: str, lines: int, status: str = "covered", auditor: str = "tester", date: str = "2026-11-26") -> str:
    return f"{path}\t{lines}\tdocs\tF2\t{auditor}\t{date}\t{status}\t#test\tnotes\n"


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.target = self.root / "tools" / "sample.py"
        self.target.parent.mkdir()
        self.target.write_text("x = 1\nx += 1\n", encoding="utf-8")
        self.ledger = self.root / "coverage.tsv"

    def _write(self, content: str) -> None:
        self.ledger.write_text(HEADER + content, encoding="utf-8")

    def test_valid_row_passes(self) -> None:
        self._write(row("tools/sample.py", 2))
        self.assertEqual(checker.validate(self.root, self.ledger, ("tools/sample.py",)), [])

    def test_missing_row_is_reported(self) -> None:
        self._write("")
        problems = checker.validate(self.root, self.ledger, ("tools/sample.py",))
        self.assertEqual(problems, ["tools/sample.py: missing from " + str(self.ledger)])

    def test_wrong_status_is_reported(self) -> None:
        self._write(row("tools/sample.py", 2, status="pending"))
        problems = checker.validate(self.root, self.ledger, ("tools/sample.py",))
        self.assertTrue(any("status is 'pending'" in p for p in problems))

    def test_stale_line_count_is_reported(self) -> None:
        self._write(row("tools/sample.py", 7))
        problems = checker.validate(self.root, self.ledger, ("tools/sample.py",))
        self.assertEqual(problems, ["tools/sample.py: ledger says 7 lines, file has 2"])

    def test_missing_auditor_and_date_are_reported(self) -> None:
        self._write(row("tools/sample.py", 2, auditor="", date=""))
        problems = checker.validate(self.root, self.ledger, ("tools/sample.py",))
        self.assertEqual(problems, ["tools/sample.py: auditor/date missing"])

    def test_missing_file_is_reported(self) -> None:
        self._write(row("tools/absent.py", 2))
        problems = checker.validate(self.root, self.ledger, ("tools/absent.py",))
        self.assertTrue(any("file does not exist" in p for p in problems))

    def test_unreadable_ledger_is_reported(self) -> None:
        problems = checker.validate(self.root, self.root / "missing.tsv", ("tools/sample.py",))
        self.assertTrue(problems[0].startswith("cannot read ledger:"))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_parity_artifacts(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("covered in audit ledger (15 files)", result.stdout)

    def test_cli_reports_a_bad_path(self) -> None:
        result = subprocess.run(
            [sys.executable, str(MODULE_PATH), "tools/does-not-exist.py"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("LEDGER:", result.stderr)


if __name__ == "__main__":
    unittest.main()
