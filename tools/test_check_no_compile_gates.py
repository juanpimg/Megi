#!/usr/bin/env python3
"""Unit tests for tools/check_no_compile_gates.py (Python only)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

MODULE_PATH = Path(__file__).with_name("check_no_compile_gates.py")
_spec = importlib.util.spec_from_file_location("check_no_compile_gates", MODULE_PATH)
runner = importlib.util.module_from_spec(_spec)
sys.modules["check_no_compile_gates"] = runner
_spec.loader.exec_module(runner)


class GateDefinitionTests(unittest.TestCase):
    def test_no_gate_invokes_cargo(self) -> None:
        for name, command in runner.GATES:
            self.assertNotIn("cargo", command, f"{name} must not compile")

    def test_all_expected_gates_are_present(self) -> None:
        names = {name for name, _ in runner.GATES}
        self.assertEqual(
            names,
            {
                "check_arch", "check_docs_links", "audit_coverage", "update_coverage",
                "parity_schema", "parity_baseline", "parity_docs", "parity_inventory",
                "parity_ledger", "deslop_doc", "real_site_snapshots", "doc_tool_refs",
                "tool_test_coverage", "real_site_manifest", "status_json",
                "work_packages", "python_syntax", "progress_consistency", "wp_board",
                "wpt_artifacts", "real_site_report", "conformance_freshness",
                "deslop_allowlist", "agent_index_refs", "workflow_scripts",
                "status_badges", "doc_images", "coverage_json", "real_site_html", "closed_work_packages", "toolchain_config", "workflow_metadata", "coverage_svg", "progress_svg", "conformance_badges", "coverage_html_areas", "fuzz_manifest", "python_tests",
            },
        )

    def test_self_test_module_is_not_included_to_avoid_recursion(self) -> None:
        self.assertNotIn("tools.test_check_no_compile_gates", runner.TEST_MODULES)

    def test_baseline_is_used_as_a_matching_report(self) -> None:
        gate = dict(runner.GATES)["parity_baseline"]
        self.assertEqual(gate[-1], runner.BASELINE)


class RunGatesTests(unittest.TestCase):
    def test_collects_exit_codes_and_output(self) -> None:
        calls: list[list[str]] = []

        def fake(command, **kwargs):
            calls.append(command)
            return SimpleNamespace(returncode=0 if len(calls) == 1 else 1, stdout="ok\n", stderr="")

        gates = (("first", ("python", "a.py")), ("second", ("python", "b.py")))
        results = runner.run_gates(Path("/repo"), gates, fake)
        self.assertEqual([item["code"] for item in results], [0, 1])
        self.assertEqual([item["name"] for item in results], ["first", "second"])
        self.assertEqual(results[0]["stdout"], "ok\n")
        self.assertEqual(calls[0], ["python", "a.py"])


class CliTests(unittest.TestCase):
    def test_real_run_passes_all_gates(self) -> None:
        result = subprocess.run([sys.executable, str(MODULE_PATH)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("37/38 compile-free gates passed", result.stdout)
        self.assertNotIn("FAIL", result.stdout)

    def test_runner_has_no_cargo_in_its_own_source(self) -> None:
        source = MODULE_PATH.read_text(encoding="utf-8")
        self.assertNotIn('"cargo"', source)


if __name__ == "__main__":
    unittest.main()
