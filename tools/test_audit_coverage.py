#!/usr/bin/env python3
"""Unit tests for tools/audit_coverage.py (audit coverage ledger)."""

from __future__ import annotations

import argparse
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("audit_coverage.py")
_spec = importlib.util.spec_from_file_location("audit_coverage", MODULE_PATH)
audit_coverage = importlib.util.module_from_spec(_spec)
sys.modules["audit_coverage"] = audit_coverage
_spec.loader.exec_module(audit_coverage)


def row(path: str, lines: int = 10, category: str = "src", phase: str = "F2", **overrides: str) -> dict[str, str]:
    values = {
        "path": path,
        "lines": str(lines),
        "category": category,
        "phase": phase,
        "auditor": "wave1",
        "date": "2026-11-16",
        "status": "covered",
        "findings": "",
        "notes": "",
    }
    values.update(overrides)
    return values


class ClassifyTests(unittest.TestCase):
    def test_representative_paths(self) -> None:
        cases = {
            "crates/html/src/tokenizer.rs": ("src", "F2"),
            "crates/html/tests/tokenizer.rs": ("test", "F1"),
            "crates/bench/src/benches.rs": ("tool", "F4"),
            "crates/shell/examples/demo.rs": ("example", "F3"),
            "fuzz/fuzz_targets/html.rs": ("fuzz", "F1"),
            "fuzz/corpus/html/a.html": ("exempt", "-"),
            "tests/html5lib-tests/tree-construction/x.dat": ("vendored", "-"),
            ".github/workflows/ci.yml": ("ci", "F3"),
            "tools/update_progress.py": ("tool", "F3"),
            "docs/ROADMAP.md": ("docs", "F3"),
            "docs/status/progress.svg": ("generated", "-"),
            "Cargo.toml": ("manifest", "F3"),
            "tests/fixtures/sample.png": ("binary", "-"),
            "Cargo.lock": ("generated", "-"),
            "tests/fixtures/sample.html": ("fixture", "F1"),
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(audit_coverage.classify(path), expected)

    def test_every_category_has_a_phase_or_is_exempt(self) -> None:
        for path in (
            "crates/css/src/lib.rs",
            "crates/css/tests/a.rs",
            "a.png",
            "Cargo.lock",
            "docs/x.md",
        ):
            category, phase = audit_coverage.classify(path)
            if category in {"binary", "vendored", "generated", "exempt"}:
                self.assertEqual(phase, "-", path)
            else:
                self.assertNotEqual(phase, "-", path)


class CountLinesTests(unittest.TestCase):
    def setUp(self) -> None:
        self._root = audit_coverage.ROOT
        self._tmp = tempfile.TemporaryDirectory()
        audit_coverage.ROOT = Path(self._tmp.name)

    def tearDown(self) -> None:
        audit_coverage.ROOT = self._root
        self._tmp.cleanup()

    def _write(self, name: str, content: bytes) -> str:
        target = Path(self._tmp.name) / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        return name

    def test_counts_a_trailing_newline_once(self) -> None:
        path = self._write("crates/a/src/lib.rs", b"one\ntwo\nthree\n")
        self.assertEqual(audit_coverage.count_lines(path), 3)

    def test_counts_a_missing_trailing_newline(self) -> None:
        path = self._write("crates/a/src/lib.rs", b"one\ntwo\nthree")
        self.assertEqual(audit_coverage.count_lines(path), 3)

    def test_empty_file_counts_zero(self) -> None:
        path = self._write("crates/a/src/lib.rs", b"")
        self.assertEqual(audit_coverage.count_lines(path), 0)

    def test_binary_files_are_not_counted(self) -> None:
        path = self._write("tests/fixtures/x.png", b"\x89PNG\r\n\x1a\n\x00")
        self.assertEqual(audit_coverage.count_lines(path), 0)


class LedgerRoundTripTests(unittest.TestCase):
    def setUp(self) -> None:
        self._ledger = audit_coverage.LEDGER
        self._stdout = sys.stdout
        self._tmp = tempfile.TemporaryDirectory()
        audit_coverage.LEDGER = Path(self._tmp.name) / "coverage.tsv"
        sys.stdout = open(Path(self._tmp.name) / "out.txt", "w", encoding="utf-8")

    def tearDown(self) -> None:
        sys.stdout.close()
        sys.stdout = self._stdout
        audit_coverage.LEDGER = self._ledger
        self._tmp.cleanup()

    def test_round_trip_keeps_every_field(self) -> None:
        rows = [
            row("crates/a/src/lib.rs", findings="#1-#3", notes="utf-8 ok"),
            row("docs/x.md", category="docs", phase="F3", status="pending", auditor="", date=""),
        ]
        audit_coverage.write_ledger(rows)
        read = audit_coverage.read_ledger()
        self.assertEqual(read, rows)

    def test_round_trip_writes_the_header(self) -> None:
        audit_coverage.write_ledger([row("crates/a/src/lib.rs")])
        first = audit_coverage.LEDGER.read_text(encoding="utf-8").splitlines()[0]
        self.assertEqual(tuple(first.split("\t")), audit_coverage.HEADER)

    def test_reading_an_empty_ledger_returns_nothing(self) -> None:
        self.assertEqual(audit_coverage.read_ledger(), [])


class CheckTests(unittest.TestCase):
    """`--check` is the CI gate: none of these states may pass."""

    def setUp(self) -> None:
        self._saved = (
            audit_coverage.LEDGER,
            audit_coverage.audited_scope,
            audit_coverage.tracked_files,
            audit_coverage.count_lines,
        )
        self._stdout, self._stderr = sys.stdout, sys.stderr
        self._tmp = tempfile.TemporaryDirectory()
        audit_coverage.LEDGER = Path(self._tmp.name) / "coverage.tsv"
        sys.stdout = open(Path(self._tmp.name) / "out.txt", "w", encoding="utf-8")
        sys.stderr = open(Path(self._tmp.name) / "err.txt", "w", encoding="utf-8")
        self._scope = [("crates/a/src/lib.rs", "src", "F2")]
        self._counts = {"crates/a/src/lib.rs": 10}
        audit_coverage.audited_scope = lambda: list(self._scope)
        audit_coverage.tracked_files = lambda: [path for path, _, _ in self._scope]
        audit_coverage.count_lines = lambda path: self._counts[path]

    def tearDown(self) -> None:
        (
            audit_coverage.LEDGER,
            audit_coverage.audited_scope,
            audit_coverage.tracked_files,
            audit_coverage.count_lines,
        ) = self._saved
        sys.stdout.close()
        sys.stderr.close()
        sys.stdout, sys.stderr = self._stdout, self._stderr
        self._tmp.cleanup()

    def _check(self, rows: list[dict[str, str]]) -> int:
        audit_coverage.write_ledger(rows)
        return audit_coverage.cmd_check(argparse.Namespace(strict=False))

    def test_accepts_a_complete_covered_ledger(self) -> None:
        self.assertEqual(self._check([row("crates/a/src/lib.rs")]), 0)

    def test_rejects_a_missing_file(self) -> None:
        self.assertEqual(self._check([]), 1)

    def test_reports_a_stale_line_count_as_drift(self) -> None:
        # The repository moves while the audit runs: a changed file is work to
        # re-read, not a reason to block an unrelated pull request.
        self.assertEqual(self._check([row("crates/a/src/lib.rs", lines=9)]), 0)

    def test_strict_mode_rejects_a_stale_line_count(self) -> None:
        audit_coverage.write_ledger([row("crates/a/src/lib.rs", lines=9)])
        self.assertEqual(audit_coverage.cmd_check(argparse.Namespace(strict=True)), 1)

    def test_rejects_category_drift(self) -> None:
        self.assertEqual(self._check([row("crates/a/src/lib.rs", category="test", phase="F1")]), 1)

    def test_rejects_covered_without_auditor(self) -> None:
        self.assertEqual(self._check([row("crates/a/src/lib.rs", auditor="")]), 1)

    def test_rejects_pending_with_auditor(self) -> None:
        rows = [row("crates/a/src/lib.rs", status="pending")]
        self.assertEqual(self._check(rows), 1)

    def test_rejects_pending_exempt_rows(self) -> None:
        self._scope = [("tests/fixtures/x.png", "binary", "-")]
        self._counts = {"tests/fixtures/x.png": 0}
        rows = [row("tests/fixtures/x.png", lines=0, category="binary", phase="-", status="pending")]
        self.assertEqual(self._check(rows), 1)

    def test_reports_rows_for_untracked_files_as_unknown(self) -> None:
        # A deleted file leaves a stale row behind; that is cleanup work for the
        # audit, and blocking every other pull request on it would get the gate
        # disabled. `--strict` keeps the audit branch honest.
        rows = [row("crates/a/src/lib.rs"), row("crates/gone/src/lib.rs")]
        self.assertEqual(self._check(rows), 0)
        audit_coverage.write_ledger(rows)
        self.assertEqual(audit_coverage.cmd_check(argparse.Namespace(strict=True)), 1)

    def test_reports_a_new_tracked_file_as_unknown(self) -> None:
        self._scope = [
            ("crates/a/src/lib.rs", "src", "F2"),
            ("crates/b/src/lib.rs", "src", "F2"),
        ]
        self._counts = {"crates/a/src/lib.rs": 10, "crates/b/src/lib.rs": 4}
        self.assertEqual(self._check([row("crates/a/src/lib.rs")]), 0)
        audit_coverage.write_ledger([row("crates/a/src/lib.rs")])
        self.assertEqual(audit_coverage.cmd_check(argparse.Namespace(strict=True)), 1)


class FuzzCorpusTest(unittest.TestCase):
    """Every fuzz target must ship at least one seed (#1425)."""

    def test_every_fuzz_target_has_a_seed(self) -> None:
        root = Path(__file__).resolve().parent.parent
        targets = root / "fuzz" / "fuzz_targets"
        corpus = root / "fuzz" / "corpus"
        self.assertTrue(targets.is_dir())
        missing = []
        for target in sorted(targets.glob("*.rs")):
            seed_dir = corpus / target.stem
            seeds = [p for p in seed_dir.iterdir() if p.is_file()] if seed_dir.is_dir() else []
            if not seeds:
                missing.append(target.stem)
        self.assertEqual(
            missing, [],
            f"fuzz targets without seed corpus: {missing}",
        )


class CiToolTestsCoverageTest(unittest.TestCase):
    """CI must execute every tools test module, not just the audit-ledger one (#1402)."""

    def test_ci_runs_all_tools_test_modules(self) -> None:
        root = Path(__file__).resolve().parent.parent
        ci = (root / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        # Find the unittest invocation line(s) in the workflow.
        lines = ci.splitlines()
        # The command may be a folded YAML scalar (`>-`) with module names on the following
        # indented lines; collect the invocation line plus its continuation block.
        start = next(
            (i for i, line in enumerate(lines) if "python3 -m unittest" in line),
            None,
        )
        self.assertIsNotNone(start, "CI must invoke python3 -m unittest")
        block: list[str] = []
        for line in lines[start:]:
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            indent = len(line) - len(line.lstrip(" "))
            # The folded-scalar command lines share the invocation's indent (10 here);
            # any line at a lower indent starts a new YAML key/step.
            if block and indent < 10:
                break
            block.append(stripped)
        joined = " ".join(block)
        for module in (
            "tools.test_audit_coverage",
            "tools.test_update_coverage",
            "tools.test_png_check",
            "tools.test_real_site_check",
        ):
            self.assertIn(module, joined, f"CI must run {module}: {joined}")
        # `test_inspector.py` is a CLI, not a test module; a broad `test_*.py` discovery
        # would try to import it. Ensure CI does not rely on that glob.
        self.assertNotIn("-p 'test_*.py'", joined)


if __name__ == "__main__":
    unittest.main()
