"""Projection of a generated Test262-subset report into progress-manifest fields.

Split out of `update_progress.py` under AGENTS.md §4 so the CLI stays a thin orchestrator.
The fields are derived from `docs/status/test262.json` (never hand-written); freshness of that
report is enforced separately by `tools/test262_report.py --check` and `tools/test262_gate.py`.
"""

from __future__ import annotations


def test262_fields(report: dict) -> dict:
    """The `[test262]` projection of a generated `megi-test262-report/1`."""
    counts = report["counts"]
    run_info = report.get("run_info") or {}
    clean = counts["pass"] == report["total"] and report.get("allowlist", 0) == 0
    return {
        "manifest_sha256": report["epoch"]["manifest_sha256"],
        "scope": run_info.get("scope", report["epoch"]["scope"]),
        **counts,
        "passed": counts["pass"],
        "total": report["total"],
        "dirs": len(run_info.get("dirs") or []),
        "allowlist": report.get("allowlist", 0),
        "headline": report["headline"],
        "status": "PASS" if clean else "FAIL",
    }
