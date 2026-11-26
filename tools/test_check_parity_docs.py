#!/usr/bin/env python3
"""Unit tests for tools/check_parity_docs.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_parity_docs.py")
_spec = importlib.util.spec_from_file_location("check_parity_docs", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_parity_docs"] = checker
_spec.loader.exec_module(checker)

TABLE = """| Sitio | Línea base | Tras system-ui + pistas legacy | Con --system-fonts |
|---|---:|---:|---:|
| example.com | 1,04 % / 1,354 | **0,92 % / 1,246** | 0,92 % / 1,246 |
| rust-lang.org | 12,02 % / 11,802 | 12,02 % / 11,802 | 10,78 % / 10,331 |
| news.ycombinator.com | 20,61 % / 29,686 | **19,97 % / 20,909** | 17,39 % / 18,653 |
| Wikipedia | 5,31 % / 5,256 | 5,31 % / 5,256 | 4,88 % / 4,911 |
"""

BASELINE = {
    "example": {"changed_pct": 0.92, "mean_abs_diff": 1.246},
    "rust": {"changed_pct": 12.02, "mean_abs_diff": 11.802},
    "hn": {"changed_pct": 19.97, "mean_abs_diff": 20.909},
    "wikipedia": {"changed_pct": 5.31, "mean_abs_diff": 5.256},
}


class ParseTests(unittest.TestCase):
    def test_parses_the_second_metric_column(self) -> None:
        parsed = checker.parse_table(TABLE)
        self.assertEqual(parsed["example"], (0.92, 1.246))
        self.assertEqual(parsed["rust"], (12.02, 11.802))
        self.assertEqual(parsed["hn"], (19.97, 20.909))
        self.assertEqual(parsed["wikipedia"], (5.31, 5.256))

    def test_ignores_header_and_unknown_rows(self) -> None:
        self.assertEqual(checker.parse_table(TABLE), checker.parse_table(TABLE + "| foo | x | y |\n"))

    def test_malformed_cell_is_a_typed_error(self) -> None:
        broken = TABLE.replace("**0,92 % / 1,246**", "n/a")
        with self.assertRaises(ValueError):
            checker.parse_table(broken)

    def test_dot_and_comma_decimals_are_supported(self) -> None:
        self.assertEqual(checker._number("0,92"), 0.92)
        self.assertEqual(checker._number("0.92"), 0.92)
        self.assertEqual(checker._number("1.234,56"), 1234.56)


class ValidateTests(unittest.TestCase):
    def test_matching_table_has_no_problems(self) -> None:
        self.assertEqual(checker.validate(TABLE, BASELINE), [])

    def test_changed_value_is_reported(self) -> None:
        broken = TABLE.replace("**0,92 % / 1,246**", "**1,92 % / 1,246**")
        problems = checker.validate(broken, BASELINE)
        self.assertEqual(problems, ["example: docs changed_pct=1.92 but baseline=0.92"])

    def test_missing_row_is_reported(self) -> None:
        broken = "\n".join(line for line in TABLE.splitlines() if "rust-lang.org" not in line)
        problems = checker.validate(broken, BASELINE)
        self.assertEqual(problems, ["rust: missing row in docs/VISUAL-PARITY.md"])

    def test_unknown_row_is_reported(self) -> None:
        extra = TABLE + "| example.com | 1 % / 1 | 1 % / 1 | 1 % / 1 |\n"
        problems = checker.validate(extra, BASELINE)
        self.assertTrue(any("docs changed_pct" in problem for problem in problems))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_versioned_files(self) -> None:
        root = Path(__file__).resolve().parent.parent
        result = subprocess.run(
            [
                sys.executable,
                str(MODULE_PATH),
                "--doc",
                str(root / "docs/VISUAL-PARITY.md"),
                "--baseline",
                str(root / "tests/fixtures/visual-parity/baseline.json"),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("docs match baseline (4 sites)", result.stdout)

    def test_cli_rejects_a_stale_doc(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            doc = root / "stale.md"
            doc.write_text(TABLE.replace("**0,92 % / 1,246**", "**9,92 % / 1,246**"), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--doc", str(doc)],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("MISMATCH", result.stderr)


if __name__ == "__main__":
    unittest.main()
