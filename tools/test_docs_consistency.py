"""Regression checks ensuring security and status docs match actual code."""

from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class DocsConsistencyTests(unittest.TestCase):
    def test_isolation_strategy_marks_ipc_opt_in_and_clarifies_forbid_unsafe(self) -> None:
        text = (ROOT / "docs/ISOLATION-STRATEGY.md").read_text(encoding="utf-8")
        self.assertNotIn("modo por defecto", text)
        self.assertNotIn("incompatible con `#![forbid(unsafe_code)]`", text)

    def test_isolation_strategy_publishes_f0_audit_section(self) -> None:
        text = (ROOT / "docs/ISOLATION-STRATEGY.md").read_text(encoding="utf-8")
        self.assertIn("Auditoría F0", text)
        for test_suite in (
            "ipc_semantic_tests",
            "file_grant_tests",
            "watchdog_tests",
            "spawn_hardening_tests",
        ):
            self.assertIn(test_suite, text)

    def test_progress_md_has_no_stale_fuzz_count(self) -> None:
        text = (ROOT / "PROGRESS.md").read_text(encoding="utf-8")
        self.assertNotIn("14/16 targets", text)


if __name__ == "__main__":
    unittest.main()
