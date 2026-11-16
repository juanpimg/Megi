#!/usr/bin/env python3
"""Generate Megi's capability-coverage status from a verifiable manifest.

The manifest `docs/status/coverage.toml` declares *known* capabilities. A capability
counts as implemented only when its `evidence` can be checked in this repository:

    gate:<suite>                     suite in [gate] of progress.toml, gate PASS, counts equal
    test:<path>::<fn>                the source file exists and contains `fn <fn>`
    fixture:<path>                   the golden/fixture file exists
    none                             only for partial/missing

`done` without verifiable evidence is an error, not a silent estimate. Adding a new
capability grows the denominator, so the percentage can legitimately go down — that
is the point of the measure (see docs/status/COVERAGE.md).

Usage:
    python3 tools/update_coverage.py --write
    python3 tools/update_coverage.py --check
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("Python 3.11+ required (tomllib).", file=sys.stderr)
    sys.exit(1)

STATUS_DIR = Path("docs/status")
MANIFEST = STATUS_DIR / "coverage.toml"
PROGRESS_MANIFEST = STATUS_DIR / "progress.toml"
ALLOWED_STATES = {"done", "partial", "missing"}


def run(root: Path, args: list[str], timeout: int = 60) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=root, capture_output=True, text=True, timeout=timeout, check=False)


def git_short_head(root: Path) -> str:
    return run(root, ["git", "rev-parse", "--short", "HEAD"]).stdout.strip() or "uncommitted"


def git_commit_date(root: Path) -> str:
    return run(root, ["git", "log", "-1", "--format=%cs"]).stdout.strip() or "unknown"


def load_manifest(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        sys.exit(f"error: manifest not found: {path} (run from the repo root)")
    except tomllib.TOMLDecodeError as error:
        sys.exit(f"error: invalid TOML in {path}: {error}")


def read_gate(root: Path) -> dict[str, str]:
    progress = load_manifest(root / PROGRESS_MANIFEST)
    return dict(progress.get("gate", {}))


def verify_evidence(cap: dict, root: Path, gate: dict[str, str]) -> tuple[bool, str]:
    """Return (verified, reason). `done` requires evidence that machines can check."""
    state = str(cap.get("state", ""))
    evidence = str(cap.get("evidence", "") or "none")
    if state not in ALLOWED_STATES:
        return False, f"estado desconocido: {state!r}"
    if state != "done":
        return True, "no requiere evidencia"
    if evidence in ("", "none"):
        return False, "state=done requiere evidencia verificable (no 'none')"

    kind, _, detail = evidence.partition(":")
    if kind == "gate":
        if str(gate.get("status", "")).upper() != "PASS":
            return False, "el gate de conformidad no está PASS"
        counts = str(gate.get(detail, ""))
        match = re.fullmatch(r"(\d+)\s*/\s*(\d+)", counts)
        if not match:
            return False, f"suite de gate desconocida o sin resultado: {detail!r}"
        if match.group(1) != match.group(2):
            return False, f"suite {detail} tiene fallos: {counts}"
        return True, f"gate {detail} {counts} PASS"

    if kind == "test":
        path_text, _, fn = detail.partition("::")
        path = root / path_text
        if not path.is_file():
            return False, f"no existe el fichero de test {path_text}"
        if not fn:
            return False, "evidence test: falta ::nombre_del_test"
        if f"fn {fn}" not in path.read_text(encoding="utf-8", errors="ignore"):
            return False, f"no aparece fn {fn} en {path_text}"
        return True, f"test {path_text}::{fn}"

    if kind == "fixture":
        path = root / detail
        if not path.is_file():
            return False, f"no existe el fixture {detail}"
        return True, f"fixture {detail} existe"

    return False, f"tipo de evidencia desconocido: {kind!r}"


def _percent(implemented: int, known: int) -> int:
    if known <= 0:
        return 0
    if implemented >= known:
        return 100
    value = round(implemented * 100 / known)
    return min(99, value)


def compute_stats(caps: list[dict], root: Path, gate: dict[str, str]) -> dict:
    """Enrich each capability with verification and compute per-area and overall counts."""
    enriched: list[dict] = []
    for raw in caps:
        item = dict(raw)
        ok, reason = verify_evidence(raw, root, gate)
        counted = str(item.get("state")) == "done" and ok
        item.update({"verified": ok, "reason": reason, "counted": counted})
        enriched.append(item)

    areas: list[dict] = []
    index: dict[str, dict] = {}
    for item in enriched:
        area_id = str(item.get("area", "?"))
        if area_id not in index:
            area = {
                "id": area_id,
                "implemented": 0,
                "partial": 0,
                "known": 0,
            }
            index[area_id] = area
            areas.append(area)
        area = index[area_id]
        area["known"] += 1
        if item["counted"]:
            area["implemented"] += 1
        elif item.get("state") == "partial":
            area["partial"] += 1
    for area in areas:
        area["percent"] = _percent(area["implemented"], area["known"])

    implemented = sum(1 for item in enriched if item["counted"])
    partial = sum(1 for item in enriched if item.get("state") == "partial")
    known = len(enriched)
    missing = [
        {
            key: item.get(key, "")
            for key in ("id", "area", "name", "source", "state", "evidence", "issue", "reason")
        }
        for item in enriched
        if not item["counted"]
    ]
    unverified = [str(item.get("id")) for item in enriched if item.get("state") == "done" and not item["verified"]]

    return {
        "areas": areas,
        "overall": {
            "implemented": implemented,
            "partial": partial,
            "known": known,
            "percent": _percent(implemented, known),
            "unverified": unverified,
        },
        "capabilities": enriched,
        "missing": missing,
    }


def build_artifacts(root: Path) -> dict[str, bytes]:
    manifest = load_manifest(root / MANIFEST)
    gate = read_gate(root)
    stats = compute_stats(manifest.get("capability", []), root, gate)
    if stats["overall"]["unverified"]:
        sys.exit(
            "error: capacidades done sin evidencia verificable: "
            + ", ".join(stats["overall"]["unverified"])
        )
    payload = {
        "schema": 1,
        "overall": stats["overall"],
        "areas": stats["areas"],
        "missing": stats["missing"],
    }
    import json

    return {
        str(MANIFEST.with_name("coverage.json")): (
            json.dumps(payload, indent=2, ensure_ascii=False) + "\n"
        ).encode("utf-8")
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Generate Megi's capability coverage (stdlib only).")
    parser.add_argument("--write", action="store_true", help="regenerate artifacts (default)")
    parser.add_argument("--check", action="store_true", help="verify artifacts are up to date")
    parser.add_argument("--root", default=None, help="repository root")
    args = parser.parse_args(argv)
    if args.write and args.check:
        parser.error("--write and --check are mutually exclusive")

    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    artifacts = build_artifacts(root)

    if args.check:
        stale = []
        for relative, content in sorted(artifacts.items()):
            path = root / relative
            if not path.is_file() or path.read_bytes() != content:
                stale.append(relative)
        if stale:
            print("coverage status is stale: " + ", ".join(stale))
            print("Run `python3 tools/update_coverage.py --write` and commit the generated files.")
            return 1
        print("coverage status is up to date")
        return 0

    written = []
    for relative, content in sorted(artifacts.items()):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != content:
            path.write_bytes(content)
            written.append(relative)
    overall = artifacts[str(MANIFEST.with_name("coverage.json"))].decode()
    import json

    percent = json.loads(overall)["overall"]
    print(
        f"coverage: {percent['implemented']}/{percent['known']} = {percent['percent']}% "
        f"({percent['partial']} partial)"
    )
    if written:
        print("wrote " + ", ".join(written))
    else:
        print("artifacts already up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
