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
TOOLS_DIR = Path(__file__).resolve().parent

# #2446: derive the module list from the repository instead of freezing an
# incomplete tuple. The only exclusions are modules that are not unit tests or
# that would recurse back into this runner; see EXCLUDED_TEST_MODULES.
EXCLUDED_TEST_MODULES = {
    "tools.test_inspector": "CLI fixture inspector, not a unittest module (CI #1402)",
    "tools.test_check_no_compile_gates": "self-reference: running this runner recursively",
}


def discover_test_modules(tools_dir: Path = TOOLS_DIR) -> tuple[str, ...]:
    """Return every importable tools.test_* unittest module, in sorted order."""
    return tuple(
        sorted(
            f"tools.{path.stem}"
            for path in tools_dir.glob("test_*.py")
            if f"tools.{path.stem}" not in EXCLUDED_TEST_MODULES
        )
    )


TEST_MODULES = discover_test_modules()
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
    ("wp_schema_docs", (PY, "tools/check_wp_schema_docs.py")),
    ("wp_generators", (PY, "tools/check_wp_generators.py")),
    ("wpt_manifest_files", (PY, "tools/check_wpt_manifest_files.py")),
    ("changelog_version", (PY, "tools/check_changelog_version.py")),
    ("wpt_report_md", (PY, "tools/check_wpt_report_md.py")),
    ("real_site_md5", (PY, "tools/check_real_site_md5.py")),
    ("wp_generator_functions", (PY, "tools/check_wp_generator_functions.py")),
    ("agents_constraints", (PY, "tools/check_agents_constraints.py")),
    # The real `--check` needs Chromium references and Megi captures that are
    # not versioned yet (#2290): wiring it here would fail on absent data and
    # fabricating either side is forbidden. Until then this named gate runs the
    # synthetic contract tests of the checker itself.
    ("window_ui", (PY, "-m", "unittest", "tools.test_check_window_ui")),
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
