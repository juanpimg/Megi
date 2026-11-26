#!/usr/bin/env python3
"""Run every compile-free gate used by the visual-parity and audit work.

No Rust compilation: only Python stdlib tools, Pillow-based checkers and
``unittest`` modules. Intended for environments with limited disk space.

Usage:
    python3 tools/check_no_compile_gates.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

PY = sys.executable
BASELINE = "tests/fixtures/visual-parity/baseline.json"
TEST_MODULES = (
    "tools.test_visual_parity",
    "tools.test_visual_parity_report",
    "tools.test_check_parity_baseline",
    "tools.test_check_parity_schema",
    "tools.test_check_parity_docs",
    "tools.test_check_parity_ledger",
    "tools.test_check_parity_inventory",
    "tools.test_check_deslop_doc",
    "tools.test_check_real_site_snapshots",
    "tools.test_check_doc_tool_refs",
    "tools.test_test_inspector",
    "tools.test_whatwg_spec",
    "tools.test_check_tool_test_coverage",
    "tools.test_check_real_site_manifest",
    "tools.test_check_status_json",
    "tools.test_check_work_packages",
    "tools.test_check_python_syntax",
    "tools.test_check_progress_consistency",
    "tools.test_check_wp_board",
    "tools.test_check_wpt_artifacts",
    "tools.test_check_real_site_report",
    "tools.test_check_conformance_freshness",
    "tools.test_check_deslop_allowlist",
    "tools.test_check_agent_index_refs",
    "tools.test_check_workflow_scripts",
    "tools.test_check_status_badges",
    "tools.test_check_doc_images",
    "tools.test_check_coverage_json",
    "tools.test_check_real_site_html",
    "tools.test_check_closed_work_packages",
    "tools.test_check_toolchain_config",
    "tools.test_check_workflow_metadata",
    "tools.test_check_coverage_svg",
    "tools.test_check_progress_svg",
    "tools.test_check_conformance_badges",
    "tools.test_check_coverage_html_areas",
    "tools.test_check_fuzz_manifest",
    "tools.test_check_fixture_links",
    "tools.test_check_test262_artifacts",
    "tools.test_check_milestone_index",
)
GATES: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("check_arch", (PY, "tools/check_arch.py")),
    ("check_docs_links", (PY, "tools/check_docs_links.py")),
    ("audit_coverage", (PY, "tools/audit_coverage.py", "--check", "--strict")),
    ("update_coverage", (PY, "tools/update_coverage.py", "--check")),
    ("parity_schema", (PY, "tools/check_parity_schema.py")),
    ("parity_baseline", (PY, "tools/check_parity_baseline.py", BASELINE)),
    ("parity_docs", (PY, "tools/check_parity_docs.py")),
    ("parity_inventory", (PY, "tools/check_parity_inventory.py")),
    ("parity_ledger", (PY, "tools/check_parity_ledger.py")),
    ("deslop_doc", (PY, "tools/check_deslop_doc.py")),
    ("real_site_snapshots", (PY, "tools/check_real_site_snapshots.py")),
    ("doc_tool_refs", (PY, "tools/check_doc_tool_refs.py")),
    ("tool_test_coverage", (PY, "tools/check_tool_test_coverage.py")),
    ("real_site_manifest", (PY, "tools/check_real_site_manifest.py")),
    ("status_json", (PY, "tools/check_status_json.py")),
    ("work_packages", (PY, "tools/check_work_packages.py")),
    ("python_syntax", (PY, "tools/check_python_syntax.py")),
    ("progress_consistency", (PY, "tools/check_progress_consistency.py")),
    ("wp_board", (PY, "tools/check_wp_board.py")),
    ("wpt_artifacts", (PY, "tools/check_wpt_artifacts.py")),
    ("real_site_report", (PY, "tools/check_real_site_report.py")),
    ("conformance_freshness", (PY, "tools/check_conformance_freshness.py")),
    ("deslop_allowlist", (PY, "tools/check_deslop_allowlist.py")),
    ("agent_index_refs", (PY, "tools/check_agent_index_refs.py")),
    ("workflow_scripts", (PY, "tools/check_workflow_scripts.py")),
    ("status_badges", (PY, "tools/check_status_badges.py")),
    ("doc_images", (PY, "tools/check_doc_images.py")),
    ("coverage_json", (PY, "tools/check_coverage_json.py")),
    ("real_site_html", (PY, "tools/check_real_site_html.py")),
    ("closed_work_packages", (PY, "tools/check_closed_work_packages.py")),
    ("toolchain_config", (PY, "tools/check_toolchain_config.py")),
    ("workflow_metadata", (PY, "tools/check_workflow_metadata.py")),
    ("coverage_svg", (PY, "tools/check_coverage_svg.py")),
    ("progress_svg", (PY, "tools/check_progress_svg.py")),
    ("conformance_badges", (PY, "tools/check_conformance_badges.py")),
    ("coverage_html_areas", (PY, "tools/check_coverage_html_areas.py")),
    ("fuzz_manifest", (PY, "tools/check_fuzz_manifest.py")),
    ("fixture_links", (PY, "tools/check_fixture_links.py")),
    ("test262_artifacts", (PY, "tools/check_test262_artifacts.py")),
    ("milestone_index", (PY, "tools/check_milestone_index.py")),
    ("python_tests", (PY, "-m", "unittest", *TEST_MODULES)),
)


def run_gates(root: Path, gates=GATES, run=subprocess.run) -> list[dict]:
    """Execute each gate and return name/exit-code/output records."""
    results: list[dict] = []
    for name, command in gates:
        completed = run(list(command), cwd=root, capture_output=True, text=True)
        results.append(
            {
                "name": name,
                "code": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            }
        )
    return results


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    results = run_gates(args.root.resolve())
    for item in results:
        status = "PASS" if item["code"] == 0 else "FAIL"
        print(f"{status} {item['name']} (exit {item['code']})")
        if item["code"] != 0:
            detail = (item["stderr"] or item["stdout"]).strip().splitlines()
            for line in detail[-6:]:
                print(f"    {line}")
    failures = sum(1 for item in results if item["code"] != 0)
    print(f"{len(results) - failures}/{len(results)} compile-free gates passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
