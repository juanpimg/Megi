#!/usr/bin/env python3
"""Unit tests for tools/check_wp_board.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_wp_board.py")
_spec = importlib.util.spec_from_file_location("check_wp_board", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_wp_board"] = checker
_spec.loader.exec_module(checker)

BOARD = """# Work Package Board

## in_progress

- WP-DEPTH-a

## review

_(empty)_

## queued

- WP-DEPTH-b

## done

_(empty)_
"""


class ParsingTests(unittest.TestCase):
    def test_parses_sections(self) -> None:
        sections = checker.parse_board(BOARD)
        self.assertEqual(sections["in_progress"], ["WP-DEPTH-a"])
        self.assertEqual(sections["queued"], ["WP-DEPTH-b"])
        self.assertEqual(sections["review"], [])
        self.assertEqual(sections["done"], [])

    def test_ignores_unknown_sections(self) -> None:
        sections = checker.parse_board("## unknown\n- WP-X-a\n## queued\n- WP-X-b\n")
        self.assertEqual(sections["queued"], ["WP-X-b"])
        self.assertEqual(sum(map(len, sections.values())), 1)


class ValidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.packages = self.root / "docs/work-packages"
        self.board = self.packages / "BOARD.md"
        self.queue = self.packages / "queue"
        self.board.parent.mkdir(parents=True)
        self.queue.mkdir()
        self.board.write_text(BOARD, encoding="utf-8")
        (self.packages / "state.json").write_text(
            json.dumps({"in_progress": ["WP-DEPTH-a"], "review": [], "done": []}),
            encoding="utf-8",
        )
        for name in ("a", "b"):
            (self.queue / f"WP-DEPTH-{name}.toml").write_text(
                f'id = "WP-DEPTH-{name}"\n', encoding="utf-8"
            )

    def test_matching_board_passes(self) -> None:
        self.assertEqual(checker.validate(self.root), [])

    def test_missing_board_or_queue_is_reported(self) -> None:
        self.board.unlink()
        self.assertTrue(checker.validate(self.root)[0].startswith("missing"))

    def test_queue_item_missing_from_board_is_reported(self) -> None:
        self.board.write_text(BOARD.replace("- WP-DEPTH-b", ""), encoding="utf-8")
        self.assertIn(
            "WP-DEPTH-b: in queue but missing from BOARD.md[queued]", checker.validate(self.root)
        )

    def test_board_item_missing_from_queue_is_reported(self) -> None:
        (self.queue / "WP-DEPTH-b.toml").unlink()
        self.assertIn(
            "WP-DEPTH-b: on BOARD.md[queued] but not an open queue package",
            checker.validate(self.root),
        )

    def test_state_mismatch_is_reported(self) -> None:
        (self.packages / "state.json").write_text(
            json.dumps({"in_progress": ["WP-DEPTH-x"], "review": [], "done": []}),
            encoding="utf-8",
        )
        problems = checker.validate(self.root)
        self.assertIn("WP-DEPTH-x: in state[in_progress] but missing from BOARD.md", problems)
        self.assertIn("WP-DEPTH-a: on BOARD.md[in_progress] but not in state", problems)

    def test_done_package_leaving_the_queue_is_accepted(self) -> None:
        (self.queue / "WP-DEPTH-a.toml").unlink()
        (self.packages / "state.json").write_text(
            json.dumps({"in_progress": [], "review": [], "done": ["WP-DEPTH-a"]}),
            encoding="utf-8",
        )
        board = BOARD.replace("- WP-DEPTH-a", "").replace("## done\n\n_(empty)_", "## done\n\n- WP-DEPTH-a")
        self.board.write_text(board, encoding="utf-8")
        (self.packages / "closed.toml").write_text('["WP-DEPTH-a"]\nid = "WP-DEPTH-a"\n', encoding="utf-8")
        self.assertEqual(checker.validate(self.root), [])

    def test_duplicate_listing_is_reported(self) -> None:
        self.board.write_text(BOARD + "\n## done\n\n- WP-DEPTH-a\n", encoding="utf-8")
        self.assertIn("WP-DEPTH-a: listed 2 times", checker.validate(self.root))

    def test_in_progress_cap_is_reported(self) -> None:
        ids = [f"WP-DEPTH-x{i}" for i in range(11)]
        (self.packages / "state.json").write_text(
            json.dumps({"in_progress": ids, "review": [], "done": []}), encoding="utf-8"
        )
        self.board.write_text(
            "## in_progress\n\n" + "\n".join(f"- {wid}" for wid in ids) + "\n",
            encoding="utf-8",
        )
        self.assertTrue(any("in_progress has 11" in p for p in checker.validate(self.root)))


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_real_board(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("work package board OK", result.stdout)

    def test_cli_rejects_an_empty_root(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [sys.executable, str(MODULE_PATH), "--root", tmp],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("WP BOARD:", result.stderr)


if __name__ == "__main__":
    unittest.main()
