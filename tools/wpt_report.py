#!/usr/bin/env python3
"""Generate the honest WPT P/T report from the real test_harness summary.

The numbers are parsed from `megi-test-harness --summary` (no hand-written P/T), the epoch comes
from the SHA-pinned WPT revision plus the byte hash of the manifest, and each subtest carries the
canonical key `sha256(test + NUL + subtest)`. The report covers all vendored fixtures actually
executed by the harness (CSS Syntax extractor plus the offline JavaScript component runner);
`scope` states that explicitly so a green local total is never mistaken for full WPT conformance.

Usage:
    python3 tools/wpt_report.py --write
    python3 tools/wpt_report.py --check   # regenerate from cargo and compare (offline)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

# #2448: support both `import tools.wpt_report` (tests/gates) and direct
# `python3 tools/wpt_report.py` / `import wpt_report` (update_progress).
try:
    from tools import wpt_gate
except ModuleNotFoundError:  # pragma: no cover - script mode
    import wpt_gate  # type: ignore[no-redef]

STATUS_DIR = Path("docs/status")
RESULT_RE = re.compile(r"^  \[(PASS|FAIL|TIMEOUT|NOTRUN|harness_error)\] (.+)$")
SECTION_RE = re.compile(r"^--- WPT (.+?) Suites ")
TOTAL_RE = re.compile(r"^(.+?) Total: (\d+) / (\d+) tests passed\s*$")
COUNT_FIELD = {"PASS": "pass", "FAIL": "fail", "TIMEOUT": "timeout",
               "NOTRUN": "notrun", "harness_error": "error"}


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def parse_summary(output: str) -> tuple[list[tuple[str, str]], int, int]:
    """Parse every WPT section of `--summary`; never infer a missing result from silence."""
    sections: list[dict] = []
    current: dict | None = None
    for line in output.splitlines():
        if line.startswith("--- WPT "):
            match = SECTION_RE.match(line)
            if match is None:
                raise ValueError(f"malformed WPT section header: {line!r}")
            current = {"name": match.group(1), "results": [], "total": None}
            sections.append(current)
            continue
        if line.startswith("---"):
            current = None
            continue
        if current is None:
            continue
        match = RESULT_RE.match(line)
        if match:
            current["results"].append((match.group(2), match.group(1)))
            continue
        total_match = TOTAL_RE.match(line)
        if total_match:
            name, passed, total = total_match.group(1), int(total_match.group(2)), int(total_match.group(3))
            if name == current["name"] and current["total"] is None:
                current["total"] = (passed, total)
    if not sections:
        raise ValueError("harness output has no WPT Suites section")
    results: list[tuple[str, str]] = []
    passed_total, tests_total = 0, 0
    for section in sections:
        if section["total"] is None:
            raise ValueError(f"WPT section {section['name']!r} has no Total line")
        declared_passed, declared_total = section["total"]
        section_results = section["results"]
        if len(section_results) != declared_total:
            raise ValueError(
                f"WPT {section['name']}: harness printed {len(section_results)} results "
                f"but declared total {declared_total}"
            )
        section_passed = sum(1 for _, status in section_results if status == "PASS")
        if section_passed != declared_passed:
            raise ValueError(f"WPT {section['name']}: passed count does not match its result lines")
        results.extend(section_results)
        passed_total += declared_passed
        tests_total += declared_total
    return results, passed_total, tests_total


def load_report(path: Path) -> dict:
    """Read a previously generated report (used by --no-gate progress generation).

    Validate the full shape consumed by progress_stats.wpt_fields(), then delegate
    to wpt_gate.load_report() (#2448) so canonical keys, counts and total are
    recomputed from results instead of trusting the published numbers. The
    wpt-specific metadata (schema/run_info) is checked here first.
    """
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error
    if not isinstance(data, dict) or data.get("schema") != "megi-wpt-report/1":
        raise ValueError(f"unsupported WPT report schema in {path}")

    def need(condition: bool, message: str) -> None:
        if not condition:
            raise ValueError(f"{path}: report needs {message}")

    # Keep the historical validation order: "epoch" is the first field reported
    # missing by tests and callers; run_info comes next.
    need(isinstance(data.get("epoch"), dict), "epoch (a dict)")
    need(isinstance(data.get("run_info"), dict), "run_info (a dict)")
    need(isinstance(data.get("run_info", {}).get("dirs"), list), "run_info.dirs (a list)")
    # #2448: wpt_gate validates epoch, results, canonical keys, counts and total.
    # Its result replaces the claimed numbers with the derived ones.
    validated = wpt_gate.load_report(path)
    data["counts"] = validated["counts"]
    data["total"] = validated["total"]
    return data


def build_report(root: Path, output: str | None = None) -> dict:
    """Run the harness (unless output is supplied) and derive the report from its output."""
    revision_path = root / "tests/wpt/WPT-REVISION.json"
    manifest_path = root / "tests/wpt/MANIFEST.json"
    try:
        revision = json.loads(revision_path.read_text(encoding="utf-8"))
        manifest_bytes = manifest_path.read_bytes()
        manifest = json.loads(manifest_bytes.decode("utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read pinned WPT inputs: {error}") from error
    pin = revision.get("revision")
    if not isinstance(pin, str) or not re.fullmatch(r"[0-9a-f]{40}", pin):
        raise ValueError("WPT-REVISION.json has no 40-hex pinned commit")
    if output is None:
        completed = subprocess.run(
            ["cargo", "run", "-q", "-p", "megi-test-harness", "--", "--summary"],
            cwd=root, capture_output=True, text=True, timeout=900, check=False,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode != 0:
            raise ValueError(f"megi-test-harness --summary exited {completed.returncode}")
    names, passed, total = parse_summary(output)
    expected = {
        entry["path"]: entry
        for entry in manifest
        if isinstance(entry, dict) and entry.get("subtest_count_expected", 0) > 0
    }
    if total != sum(entry["subtest_count_expected"] for entry in expected.values()):
        raise ValueError(f"harness total {total} disagrees with the manifest's adopted subtests")
    results = []
    for name, status in names:
        filename = name.split(":", 1)[0]
        if filename.endswith(" (newline normalization)"):
            filename = filename.replace(" (newline normalization)", "")
        matches = [path for path in expected if path == filename or path.endswith("/" + filename)]
        if len(matches) != 1:
            raise ValueError(f"result {name!r} does not map to exactly one manifest fixture")
        test = "tests/wpt/" + matches[0]
        results.append(
            {
                "key": sha256_text(f"{test}\0{name}"),
                "test": test,
                "subtest": name,
                "status": status,
            }
        )
    counts = {field: 0 for field in ("pass", "fail", "timeout", "notrun", "error")}
    for result in results:
        counts[COUNT_FIELD[result["status"]]] += 1
    dirs = sorted({str(Path(r["test"]).parent) for r in results})
    harness = "megi-test-harness --summary (CSS Syntax extractor + offline JS component runner)"
    scope = (
        f"{len(expected)} vendored WPT fixtures ({total} executed subtests, "
        f"{len(dirs)} dirs); NOT full WPT conformance"
    )
    return {
        "schema": "megi-wpt-report/1",
        "epoch": {
            "wpt_revision": pin,
            "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
        },
        "run_info": {
            "harness": harness,
            "scope": scope,
            "adopted_subtests": total,
            "dirs": dirs,
        },
        "counts": counts,
        "total": total,
        "results": results,
    }


def render_markdown(report: dict) -> str:
    c = report["counts"]
    epoch = report["epoch"]
    lines = [
        "# WPT honest P/T (generated)\n\n",
        "<!-- Generated by tools/wpt_report.py — do not edit by hand. -->\n\n",
        f"WPT {epoch['wpt_revision'][:7]} — {c['pass']}/{report['total']} subtests pasados "
        f"(dirs: {len(report['run_info']['dirs'])})\n\n",
        "| Status | Subtests |\n|---|---:|\n",
    ]
    for field, label in (("pass", "PASS"), ("fail", "FAIL"), ("timeout", "TIMEOUT"),
                         ("notrun", "NOTRUN"), ("error", "harness_error")):
        lines.append(f"| {label} | {c[field]} |\n")
    lines.append(
        f"\nEpoch: WPT commit `{epoch['wpt_revision']}` · "
        f"manifest SHA-256 `{epoch['manifest_sha256']}`.\n\n"
    )
    lines.append(
        f"Scope: {report['run_info']['scope']}. Capability coverage and WPT conformance are "
        "separate metrics; they are never merged into one percentage.\n"
    )
    return "".join(lines)


def write_report(root: Path, report: dict) -> None:
    target = root / STATUS_DIR
    target.mkdir(parents=True, exist_ok=True)
    (target / "wpt.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    (target / "wpt.md").write_text(render_markdown(report), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--write", action="store_true", help="regenerate docs/status/wpt.* (default)")
    group.add_argument("--check", action="store_true", help="fail if generated artifacts differ")
    parser.add_argument("--root", default="", help="repository root (default: parent of tools/)")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    try:
        report = build_report(root)
        json_bytes = (json.dumps(report, indent=2) + "\n").encode("utf-8")
        md_bytes = render_markdown(report).encode("utf-8")
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if args.check:
        stale = []
        for name, generated in (("wpt.json", json_bytes), ("wpt.md", md_bytes)):
            artifact = root / STATUS_DIR / name
            if not artifact.is_file() or artifact.read_bytes() != generated:
                stale.append(name)
        if stale:
            print("stale: " + ", ".join(stale), file=sys.stderr)
            return 1
        print(f"wpt_report: up to date ({report['counts']['pass']}/{report['total']})")
        return 0
    write_report(root, report)
    print(f"wpt_report: wrote wpt.json/wpt.md ({report['counts']['pass']}/{report['total']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
