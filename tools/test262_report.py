#!/usr/bin/env python3
"""Publish the honest Test262-subset P/T report from the real harness.

The numbers are parsed from `megi-test-harness --test262` (never hand-written), the epoch is the
SHA-256 of the pinned manifest plus its declared scope, and every case carries the canonical key
`sha256(test + NUL + subtest)` plus its exact fixture pin. Before generating anything, every
fixture byte is verified against `tests/test262/MANIFEST.json`.

`scope` is always "NOT full Test262 conformance": the curated set is a language-expressions smoke
subset, not a vendored copy of tc39/test262.

Usage:
    python3 tools/test262_report.py --write
    python3 tools/test262_report.py --check   # regenerate and compare (offline)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

STATUS_DIR = Path("docs/status")
MANIFEST_REL = "tests/test262/MANIFEST.json"
MANIFEST_SCHEMA = "megi-test262-manifest/1"
REPORT_SCHEMA = "megi-test262-report/1"
SECTION_RE = re.compile(r"^--- Test262 (.+?) Suites ")
RESULT_RE = re.compile(r"^  \[(PASS|FAIL|TIMEOUT|NOTRUN|harness_error)\] (.+)$")
TOTAL_RE = re.compile(r"^(.+?) Total: (\d+) / (\d+) tests passed\s*$")
GRAND_RE = re.compile(r"^Test262 subset: (\d+) / (\d+) cases passed\s*$")
COUNT_FIELD = {
    "PASS": "pass", "FAIL": "fail", "TIMEOUT": "timeout",
    "NOTRUN": "notrun", "harness_error": "error",
}


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def load_manifest(root: Path) -> dict:
    """Read and validate the curated manifest, then verify every pinned fixture byte."""
    path = root / MANIFEST_REL
    try:
        raw = path.read_bytes()
        manifest = json.loads(raw.decode("utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read pinned Test262 inputs: {error}") from error
    if not isinstance(manifest, dict) or manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValueError(f"unsupported Test262 manifest schema in {path}")
    scope = manifest.get("scope")
    if not isinstance(scope, str) or "NOT full Test262" not in scope:
        raise ValueError("Test262 manifest must declare 'NOT full Test262' in scope")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("Test262 manifest has no cases")
    base = root / "tests/test262"
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            raise ValueError(f"cases[{index}] must be an object")
        for field in ("path", "sha256", "dir"):
            if not isinstance(case.get(field), str):
                raise ValueError(f"cases[{index}] needs a string {field}")
        if not isinstance(case.get("bytes"), int) or isinstance(case.get("bytes"), bool):
            raise ValueError(f"cases[{index}] needs an integer bytes")
        fixture = base / case["path"]
        try:
            data = fixture.read_bytes()
        except OSError as error:
            raise ValueError(f"cannot read {fixture}: {error}") from error
        digest = hashlib.sha256(data).hexdigest()
        if digest != case["sha256"] or len(data) != case["bytes"]:
            raise ValueError(
                f"pin mismatch for {case['path']}: sha256 {digest}, bytes {len(data)}"
            )
    return {
        "scope": scope,
        "source": manifest.get("source", ""),
        "adopted_cases": len(cases),
        "cases": cases,
        "manifest_sha256": hashlib.sha256(raw).hexdigest(),
    }


def parse_summary(output: str) -> tuple[list[tuple[str, str]], int, int]:
    """Parse every `--- Test262 ... ---` section; never infer a result from silence."""
    sections: list[dict] = []
    current: dict | None = None
    grand: tuple[int, int] | None = None
    for line in output.splitlines():
        if line.startswith("--- Test262 "):
            match = SECTION_RE.match(line)
            if match is None:
                raise ValueError(f"malformed Test262 section header: {line!r}")
            current = {"name": match.group(1), "results": [], "total": None}
            sections.append(current)
            continue
        if line.startswith("---"):
            current = None
            continue
        grand_match = GRAND_RE.match(line)
        if grand_match:
            grand = (int(grand_match.group(1)), int(grand_match.group(2)))
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
        raise ValueError("harness output has no Test262 Suites section")
    if grand is None:
        raise ValueError("harness output has no 'Test262 subset' total line")
    results: list[tuple[str, str]] = []
    passed_total, tests_total = 0, 0
    for section in sections:
        if section["total"] is None:
            raise ValueError(f"Test262 section {section['name']!r} has no Total line")
        declared_passed, declared_total = section["total"]
        if len(section["results"]) != declared_total:
            raise ValueError(
                f"Test262 {section['name']}: harness printed {len(section['results'])} results "
                f"but declared total {declared_total}"
            )
        section_passed = sum(1 for _, status in section["results"] if status == "PASS")
        if section_passed != declared_passed:
            raise ValueError(f"Test262 {section['name']}: passed count does not match result lines")
        results.extend(section["results"])
        passed_total += declared_passed
        tests_total += declared_total
    if grand != (passed_total, tests_total):
        raise ValueError(f"Test262 subset total {grand} disagrees with sections {(passed_total, tests_total)}")
    return results, passed_total, tests_total


def render_headline(report: dict) -> str:
    counts, epoch, run_info = report["counts"], report["epoch"], report["run_info"]
    parts = [
        f"{counts[field]} {label}"
        for field, label in (("fail", "FAIL"), ("timeout", "TIMEOUT"),
                             ("notrun", "NOTRUN"), ("error", "harness_error"))
        if counts.get(field, 0)
    ]
    headline = (
        f"Test262 {epoch['manifest_sha256'][:7]} — "
        f"{counts['pass']}/{report['total']} cases passed "
        f"(dirs: {len(run_info['dirs'])}"
    )
    if parts:
        headline += "; " + ", ".join(parts)
    return headline + ")"


def build_report(root: Path, output: str | None = None) -> dict:
    manifest = load_manifest(root)
    if output is None:
        completed = subprocess.run(
            ["cargo", "run", "-q", "-p", "megi-test-harness", "--", "--test262"],
            cwd=root, capture_output=True, text=True, timeout=900, check=False,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode != 0:
            raise ValueError(f"megi-test-harness --test262 exited {completed.returncode}")
    names, passed, total = parse_summary(output)
    by_path = {case["path"]: case for case in manifest["cases"]}
    if total != len(by_path) or passed != total:
        raise ValueError(
            f"harness reported {passed}/{total} but the manifest pins {len(by_path)} cases"
        )
    if len({name for name, _ in names}) != len(names):
        raise ValueError("harness printed a duplicate case")
    results = []
    for name, status in names:
        case = by_path.get(name)
        if case is None:
            raise ValueError(f"result {name!r} is not a pinned Test262 case")
        test = "tests/test262/" + name
        results.append({
            "key": sha256_text(f"{test}\0{name}"),
            "test": test,
            "subtest": name,
            "status": status,
            "sha256": case["sha256"],
        })
    counts = {field: 0 for field in ("pass", "fail", "timeout", "notrun", "error")}
    for result in results:
        counts[COUNT_FIELD[result["status"]]] += 1
    dirs = sorted({case["dir"] for case in manifest["cases"]})
    scope = manifest["scope"]
    report = {
        "schema": REPORT_SCHEMA,
        "epoch": {
            "manifest_sha256": manifest["manifest_sha256"],
            "scope": scope,
        },
        "run_info": {
            "harness": "megi-test-harness --test262 (offline Boa assert shim)",
            "scope": scope,
            "source": manifest["source"],
            "adopted_cases": manifest["adopted_cases"],
            "dirs": dirs,
        },
        "counts": counts,
        "total": len(results),
        "results": results,
        "allowlist": 0,
    }
    report["headline"] = render_headline(report)
    return report


def render_markdown(report: dict) -> str:
    counts, epoch, run_info = report["counts"], report["epoch"], report["run_info"]
    lines = [
        "# Test262 honest P/T (generated)\n\n",
        "<!-- Generated by tools/test262_report.py — do not edit by hand. -->\n\n",
        f"{report['headline']}\n\n",
        "| Status | Cases |\n|---|---:|\n",
    ]
    for field, label in (("pass", "PASS"), ("fail", "FAIL"), ("timeout", "TIMEOUT"),
                         ("notrun", "NOTRUN"), ("error", "harness_error")):
        lines.append(f"| {label} | {counts[field]} |\n")
    lines.append(
        f"\nEpoch: manifest SHA-256 `{epoch['manifest_sha256']}` · "
        f"scope `{epoch['scope']}` · allowlist `{report['allowlist']}`.\n\n"
    )
    lines.append(
        f"Scope: {run_info['scope']}. Capability coverage and Test262 conformance are separate "
        "metrics; they are never merged into one percentage.\n"
    )
    return "".join(lines)


def load_report(path: Path) -> dict:
    """Read a previously generated report with the full shape consumed by progress_stats."""
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("schema") != REPORT_SCHEMA:
        raise ValueError(f"unsupported Test262 report schema in {path}")

    def need(condition: bool, message: str) -> None:
        if not condition:
            raise ValueError(f"{path}: report needs {message}")

    epoch, counts = data.get("epoch"), data.get("counts")
    need(isinstance(epoch, dict), "epoch (a dict)")
    need(isinstance(counts, dict), "counts (a dict)")
    need(isinstance(data.get("results"), list), "results (a list)")
    need(isinstance(data.get("run_info"), dict), "run_info (a dict)")
    need(isinstance(data.get("total"), int) and not isinstance(data.get("total"), bool), "total (an int)")
    need(isinstance(data.get("headline"), str), "headline (a str)")
    need(isinstance(epoch.get("manifest_sha256"), str), "epoch.manifest_sha256 (a str)")
    need(isinstance(epoch.get("scope"), str), "epoch.scope (a str)")
    need(isinstance(counts.get("pass"), int) and not isinstance(counts.get("pass"), bool), "counts.pass (an int)")
    need(isinstance(data.get("run_info", {}).get("dirs"), list), "run_info.dirs (a list)")
    return data


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--write", action="store_true", help="regenerate docs/status/test262.* (default)")
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
        for name, generated in (("test262.json", json_bytes), ("test262.md", md_bytes)):
            artifact = root / STATUS_DIR / name
            if not artifact.is_file() or artifact.read_bytes() != generated:
                stale.append(name)
        if stale:
            print("stale: " + ", ".join(stale), file=sys.stderr)
            return 1
        print(f"test262_report: up to date ({report['counts']['pass']}/{report['total']})")
        return 0
    target = root / STATUS_DIR
    target.mkdir(parents=True, exist_ok=True)
    (target / "test262.json").write_bytes(json_bytes)
    (target / "test262.md").write_bytes(md_bytes)
    print(f"test262_report: wrote test262.json/test262.md ({report['counts']['pass']}/{report['total']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
