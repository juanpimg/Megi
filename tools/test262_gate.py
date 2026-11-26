#!/usr/bin/env python3
"""Monotonic Test262-subset gate: report vs baseline + allowlist.

The epoch is `(manifest_sha256, scope)`. The gate rejects an epoch change, an unallowlisted
failure, a regression of a baseline key, a shrinking total/passed count, a growing allowlist and
any wildcard. `rebaseline` is the only way to accept a new epoch. With all adopted cases passing
the allowlist is empty — this is the honest M9 "allowlist = 0" evidence for the declared subset.

Usage:
    python3 tools/test262_gate.py --check
    python3 tools/test262_gate.py --rebaseline
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

REPORT = Path("docs/status/test262.json")
BASELINE = Path("tests/test262/baseline.json")
ALLOWLIST = Path("tests/test262/allowlist.toml")
SCHEMA = "megi-test262-report/1"
RANK = {"harness_error": 0, "NOTRUN": 1, "TIMEOUT": 2, "FAIL": 3, "PASS": 4}
FIELD = {"harness_error": "error", "NOTRUN": "notrun", "TIMEOUT": "timeout",
         "FAIL": "fail", "PASS": "pass"}
ALLOW_FIELDS = ("key", "test", "subtest", "reason", "issue", "spec")
WILDCARD = re.compile(r"[*?\[\]]")


def canonical_key(test: str, subtest: str) -> str:
    return hashlib.sha256(f"{test}\0{subtest}".encode("utf-8")).hexdigest()


def _read_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error


def _epoch(value, where: str) -> dict[str, str]:
    if not isinstance(value, dict):
        raise ValueError(f"{where}: epoch must be an object")
    manifest, scope = value.get("manifest_sha256"), value.get("scope")
    if not isinstance(manifest, str) or not manifest or not isinstance(scope, str):
        raise ValueError(f"{where}: epoch needs manifest_sha256 and scope")
    return {"manifest_sha256": manifest, "scope": scope}


def load_report(path: Path) -> dict:
    payload = _read_json(path)
    if not isinstance(payload, dict) or payload.get("schema") != SCHEMA:
        raise ValueError(f"{path}: unsupported Test262 report schema")
    results = payload.get("results")
    if not isinstance(results, list):
        raise ValueError(f"{path}: results must be an array")
    counts = {field: 0 for field in ("pass", "fail", "timeout", "notrun", "error")}
    seen: set[str] = set()
    for index, result in enumerate(results):
        if not isinstance(result, dict):
            raise ValueError(f"{path}: results[{index}] must be an object")
        test, subtest = result.get("test"), result.get("subtest")
        if not isinstance(test, str) or not isinstance(subtest, str):
            raise ValueError(f"{path}: results[{index}] needs string test/subtest")
        status = result.get("status")
        if status not in RANK:
            raise ValueError(f"{path}: results[{index}] has unknown status {status!r}")
        key = canonical_key(test, subtest)
        if result.get("key") != key:
            raise ValueError(f"{path}: results[{index}] key does not match sha256(test + NUL + subtest)")
        if key in seen:
            raise ValueError(f"{path}: duplicate result key {key}")
        seen.add(key)
        counts[FIELD[status]] += 1
    if payload.get("counts") != counts:
        raise ValueError(f"{path}: counts do not match results")
    if payload.get("total") != len(results):
        raise ValueError(f"{path}: total does not match results")
    return {"epoch": _epoch(payload.get("epoch"), str(path)), "counts": counts,
            "total": len(results), "results": results}


def load_baseline(path: Path) -> dict:
    payload = _read_json(path)
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
    return {"epoch": _epoch(payload.get("epoch"), str(path)),
            "passed": passed, "total": total, "allowlist": allowlist}


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
    if report["epoch"] != baseline["epoch"]:
        return "epoch changed (manifest SHA or scope); rebaseline required"
    statuses = {
        canonical_key(str(r["test"]), str(r["subtest"])): r["status"]
        for r in report["results"]
    }
    entries = {entry["key"]: entry for entry in allowlist}
    if len(entries) != len(allowlist):
        return "duplicate allowlist key"
    for entry in allowlist:
        for field in ("key", "test", "subtest"):
            if WILDCARD.search(entry[field]):
                return f"wildcard in allowlist {field}: {entry[field]!r}"
    baseline_keys = {entry["key"] for entry in baseline["allowlist"]}
    if len(allowlist) > len(baseline["allowlist"]):
        return "allowlist grew beyond the baseline"
    if set(entries) - baseline_keys:
        return "new allowlist key(s)"
    for key, status in sorted(statuses.items()):
        if status != "PASS" and key not in entries:
            return f"failure not allowlisted: {key} ({status})"
    for entry in baseline["allowlist"]:
        key = entry["key"]
        if key not in statuses:
            return f"baseline key no longer present: {key}"
        old = entry.get("status", "FAIL")
        if RANK.get(old, 3) > RANK[statuses[key]]:
            return f"baseline key regressed: {key} ({old} -> {statuses[key]})"
    if report["counts"]["pass"] < baseline["passed"]:
        return f"passed regressed: {report['counts']['pass']} < {baseline['passed']}"
    if report["total"] < baseline["total"]:
        return f"total regressed: {report['total']} < {baseline['total']}"
    return None


def rebaseline(report: dict, baseline_path: Path, allowlist_path: Path) -> None:
    failures = [
        {"key": canonical_key(str(r["test"]), str(r["subtest"])),
         "test": r["test"], "subtest": r["subtest"], "status": r["status"]}
        for r in report["results"] if r["status"] != "PASS"
    ]
    baseline = {"epoch": report["epoch"], "passed": report["counts"]["pass"],
                "total": report["total"], "allowlist": failures}
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
    lines = [
        "# Generated by tools/test262_gate.py --rebaseline — review reason/issue/spec before accepting.\n",
        "# Empty reason/issue/spec means a fresh, unreviewed failure: close it with a spec cite.\n",
    ]
    for failure in failures:
        lines.append("\n[[allow]]\n")
        for field in ALLOW_FIELDS:
            lines.append(f"{field} = {json.dumps(str(failure.get(field, '')))}\n")
    allowlist_path.parent.mkdir(parents=True, exist_ok=True)
    allowlist_path.write_text("".join(lines), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="verify report against baseline")
    group.add_argument("--rebaseline", action="store_true", help="accept current epoch as baseline")
    parser.add_argument("--report", default=str(REPORT))
    parser.add_argument("--baseline", default=str(BASELINE))
    parser.add_argument("--allowlist", default=str(ALLOWLIST))
    args = parser.parse_args(argv)
    try:
        report = load_report(Path(args.report))
        if args.rebaseline:
            rebaseline(report, Path(args.baseline), Path(args.allowlist))
            print(f"test262_gate: rebaselined {report['counts']['pass']}/{report['total']}")
            return 0
        error = check_gate(report, load_baseline(Path(args.baseline)),
                           load_allowlist(Path(args.allowlist)))
    except ValueError as problem:
        print(f"error: {problem}", file=sys.stderr)
        return 2
    if error:
        print(f"test262_gate: FAIL — {error}", file=sys.stderr)
        return 1
    print(f"test262_gate: PASS — {report['counts']['pass']}/{report['total']} cases, allowlist clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
