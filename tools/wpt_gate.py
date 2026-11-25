#!/usr/bin/env python3
"""Monotonic WPT conformance gate: report vs baseline + allowlist.

An epoch is `(wpt_revision, sha256(manifest_bytes))`. The gate rejects an epoch change,
a regression of any baseline key, a shrinking denominator/passed count, a growing or
new allowlist key, a wildcard in the allowlist, and any failure that is not explicitly
allowlisted. `rebaseline` is the only way to accept a new epoch.

Usage:
    python3 tools/wpt_gate.py --check --report docs/status/wpt.json \
        --baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml
    python3 tools/wpt_gate.py --rebaseline --report docs/status/wpt.json \
        --baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

STATUS_RANK = {"harness_error": 0, "NOTRUN": 1, "TIMEOUT": 2, "FAIL": 3, "PASS": 4}
COUNT_FIELD = {
    "harness_error": "error", "NOTRUN": "notrun", "TIMEOUT": "timeout",
    "FAIL": "fail", "PASS": "pass",
}
ALLOW_FIELDS = ("key", "test", "subtest", "reason", "issue", "spec")
WILDCARD = re.compile(r"[*?\[\]]")


def canonical_key(test: str, subtest: str) -> str:
    return hashlib.sha256(f"{test}\0{subtest}".encode("utf-8")).hexdigest()


def _load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error


def _epoch(value: object, where: str) -> dict[str, str]:
    if not isinstance(value, dict):
        raise ValueError(f"{where}: epoch must be an object")
    revision, manifest = value.get("wpt_revision"), value.get("manifest_sha256")
    if not isinstance(revision, str) or not isinstance(manifest, str) or not manifest:
        raise ValueError(f"{where}: epoch needs wpt_revision and manifest_sha256")
    return {"wpt_revision": revision, "manifest_sha256": manifest}


def load_report(path: Path) -> dict:
    """Load a wptreport; counts are always derived from results, never trusted as claims."""
    payload = _load_json(path)
    if not isinstance(payload, dict):
        raise ValueError(f"{path}: report must be an object")
    epoch = _epoch(payload.get("epoch"), str(path))
    results = payload.get("results")
    if not isinstance(results, list):
        raise ValueError(f"{path}: results must be an array")
    counts = {field: 0 for field in ("pass", "fail", "timeout", "notrun", "error")}
    seen: set[str] = set()
    for index, result in enumerate(results):
        if not isinstance(result, dict):
            raise ValueError(f"{path}: results[{index}] must be an object")
        for field in ("test", "subtest"):
            if not isinstance(result.get(field), str):
                raise ValueError(f"{path}: results[{index}] needs string {field}")
        status = result.get("status")
        if status not in STATUS_RANK:
            raise ValueError(f"{path}: results[{index}] has unknown status {status!r}")
        key = canonical_key(str(result.get("test", "")), str(result.get("subtest", "")))
        if result.get("key") != key:
            raise ValueError(f"{path}: results[{index}] key does not match sha256(test + NUL + subtest)")
        if key in seen:
            raise ValueError(f"{path}: duplicate result key {key}")
        seen.add(key)
        counts[COUNT_FIELD[status]] += 1
    claimed = payload.get("counts")
    if claimed != counts:
        raise ValueError(f"{path}: counts do not match results (claimed {claimed}, derived {counts})")
    total = payload.get("total")
    if total != len(results):
        raise ValueError(f"{path}: total does not match results (claimed {total}, actual {len(results)})")
    return {"epoch": epoch, "counts": counts, "total": len(results), "results": results}


def load_baseline(path: Path) -> dict:
    payload = _load_json(path)
    if not isinstance(payload, dict):
        raise ValueError(f"{path}: baseline must be an object")
    allowlist = payload.get("allowlist")
    if not isinstance(allowlist, list):
        raise ValueError(f"{path}: baseline allowlist must be an array")
    passed, total = payload.get("passed"), payload.get("total")
    if type(passed) is not int or type(total) is not int:
        raise ValueError(f"{path}: baseline passed/total must be integers")
    for index, entry in enumerate(allowlist):
        if not isinstance(entry, dict) or not isinstance(entry.get("key"), str):
            raise ValueError(f"{path}: baseline allowlist[{index}] needs a string key")
    return {
        "epoch": _epoch(payload.get("epoch"), str(path)),
        "passed": passed, "total": total, "allowlist": allowlist,
    }


def load_allowlist(path: Path) -> list[dict]:
    try:
        payload = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error
    entries = payload.get("allow", [])
    if not isinstance(entries, list):
        raise ValueError(f"{path}: allowlist must use [[allow]] tables")
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            raise ValueError(f"{path}: allow[{index}] must be a table")
        for field in ALLOW_FIELDS:
            if not isinstance(entry.get(field), str):
                raise ValueError(f"{path}: allow[{index}] needs a string {field}")
    return entries


def check_gate(report: dict, baseline: dict, allowlist: list[dict]) -> str | None:
    """Return a gate failure message, or None when the epoch is green."""
    if report["epoch"] != baseline["epoch"]:
        return "epoch changed (WPT SHA or manifest bytes); rebaseline required"
    statuses: dict[str, str] = {}
    for result in report["results"]:
        key = canonical_key(str(result["test"]), str(result["subtest"]))
        statuses[key] = result["status"]
    entries = {entry["key"]: entry for entry in allowlist}
    if len(entries) != len(allowlist):
        return "duplicate allowlist key"
    for entry in allowlist:
        for field in ("key", "test", "subtest"):
            if WILDCARD.search(entry[field]):
                return f"wildcard in allowlist {field}: {entry[field]!r}"
    baseline_keys = {entry.get("key") for entry in baseline["allowlist"]}
    if len(allowlist) > len(baseline["allowlist"]):
        return "allowlist grew beyond the baseline"
    new_keys = set(entries) - baseline_keys
    if new_keys:
        return f"new allowlist key(s): {sorted(new_keys)}"
    for key, status in sorted(statuses.items()):
        if status != "PASS" and key not in entries:
            return f"failure not allowlisted: {key} ({status})"
    for entry in baseline["allowlist"]:
        key = entry.get("key")
        if key not in statuses:
            return f"baseline key no longer present: {key}"
        old = entry.get("status", "FAIL")
        if STATUS_RANK.get(old, 3) > STATUS_RANK[statuses[key]]:
            return f"baseline key regressed: {key} ({old} -> {statuses[key]})"
    if report["counts"]["pass"] < baseline["passed"]:
        return f"passed regressed: {report['counts']['pass']} < {baseline['passed']}"
    if report["total"] < baseline["total"]:
        return f"total regressed: {report['total']} < {baseline['total']}"
    return None


def rebaseline(report: dict, baseline_path: Path, allowlist_path: Path) -> None:
    """Write the current epoch as baseline; new failures enter the allowlist with empty reasons."""
    failures = [
        {"key": canonical_key(str(r["test"]), str(r["subtest"])),
         "test": r["test"], "subtest": r["subtest"], "status": r["status"]}
        for r in report["results"] if r["status"] != "PASS"
    ]
    baseline = {
        "epoch": report["epoch"],
        "passed": report["counts"]["pass"],
        "total": report["total"],
        "allowlist": failures,
    }
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    lines = [
        "# Generated by tools/wpt_gate.py --rebaseline — review reason/issue/spec before accepting.\n",
        "# Empty reason/issue/spec means a fresh, unreviewed failure: close it with a spec cite.\n",
    ]
    for failure in failures:
        lines.append("\n[[allow]]\n")
        for field in ALLOW_FIELDS:
            value = failure.get(field, "")
            lines.append(f"{field} = {json.dumps(str(value))}\n")
    allowlist_path.parent.mkdir(parents=True, exist_ok=True)
    allowlist_path.write_text("".join(lines), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="verify report against baseline")
    group.add_argument("--rebaseline", action="store_true", help="accept current epoch as baseline")
    parser.add_argument("--report", default="docs/status/wpt.json")
    parser.add_argument("--baseline", default="tests/wpt/baseline.json")
    parser.add_argument("--allowlist", default="tests/wpt/allowlist.toml")
    args = parser.parse_args(argv)
    try:
        report = load_report(Path(args.report))
        if args.rebaseline:
            rebaseline(report, Path(args.baseline), Path(args.allowlist))
            print(f"wpt_gate: rebaselined {report['counts']['pass']}/{report['total']}")
            return 0
        error = check_gate(report, load_baseline(Path(args.baseline)), load_allowlist(Path(args.allowlist)))
    except ValueError as problem:
        print(f"error: {problem}", file=sys.stderr)
        return 2
    if error:
        print(f"wpt_gate: FAIL — {error}", file=sys.stderr)
        return 1
    print(f"wpt_gate: PASS — {report['counts']['pass']}/{report['total']} subtests, allowlist clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
