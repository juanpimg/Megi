#!/usr/bin/env python3
"""Unit tests for tools/check_deslop_doc.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_deslop_doc.py")
_spec = importlib.util.spec_from_file_location("check_deslop_doc", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_deslop_doc"] = checker
_spec.loader.exec_module(checker)

DOC = """## Baseline de deuda (2026-11-25)

El primer escaneo completo sobre `origin/main` produjo **219 hallazgos**:

| Regla | Hallazgos |
|---|---:|
| `field-leak` | 119 |
| `module-cycle` | 53 |
| `tangle` | 17 |
| `shallow-module` | 15 |
| `wide-directory` | 15 |
| `long-file` | 0 |

### Delta verificado (2026-11-26)

El allowlist pasó de **219 a 215 entradas**: se eliminaron 8 y se añadieron 4.
"""

ALLOW = (
    "field-leak a\n" * 114
    + "module-cycle a\n" * 53
    + "tangle a\n" * 17
    + "shallow-module a\n" * 16
    + "wide-directory a\n" * 15
    + "# comment\n"
)


class ParsingTests(unittest.TestCase):
    def test_baseline_table(self) -> None:
        table = checker.parse_baseline_table(DOC)
        self.assertEqual(sum(table.values()), 219)
        self.assertEqual(table["field-leak"], 119)
        self.assertEqual(table["long-file"], 0)

    def test_claim(self) -> None:
        self.assertEqual(checker.parse_claim(DOC), (219, 215, 8, 4))

    def test_missing_claim_is_a_typed_error(self) -> None:
        with self.assertRaises(ValueError):
            checker.parse_claim("no figures here")

    def test_allowlist_counts_ignore_comments(self) -> None:
        counts = checker.count_allowlist(ALLOW)
        self.assertEqual(sum(counts.values()), 215)
        self.assertEqual(counts["field-leak"], 114)


class ValidateTests(unittest.TestCase):
    def test_matching_doc_and_allowlist_pass(self) -> None:
        self.assertEqual(checker.validate(DOC, ALLOW), [])

    def test_wrong_allowlist_total_is_reported(self) -> None:
        problems = checker.validate(DOC, ALLOW + "field-leak extra\n")
        self.assertTrue(any("allowlist has 216 entries" in p for p in problems))

    def test_broken_delta_arithmetic_is_reported(self) -> None:
        broken = DOC.replace("se eliminaron 8", "se eliminaron 9")
        problems = checker.validate(broken, ALLOW)
        self.assertTrue(any("delta inconsistent" in p for p in problems))

    def test_missing_table_row_is_reported(self) -> None:
        broken = DOC.replace("| `long-file` | 0 |\n", "")
        problems = checker.validate(broken, ALLOW)
        self.assertIn("baseline table has no row for long-file", problems)

    def test_wrong_table_total_is_reported(self) -> None:
        broken = DOC.replace("| `field-leak` | 119 |", "| `field-leak` | 109 |")
        problems = checker.validate(broken, ALLOW)
        self.assertTrue(any("baseline table sums to 209" in p for p in problems))

    def test_unknown_rule_is_reported(self) -> None:
        problems = checker.validate(DOC, ALLOW + "mystery entry\n")
        self.assertTrue(any("unknown rule 'mystery'" in p for p in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_real_files(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("matches allowlist (215 entries)", result.stdout)

    def test_cli_reports_unreadable_allowlist(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--allow", str(Path(tmp) / "missing")],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("check_deslop_doc:", result.stderr)


if __name__ == "__main__":
    unittest.main()
