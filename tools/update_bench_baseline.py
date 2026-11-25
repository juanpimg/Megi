#!/usr/bin/env python3
"""Publish a numbered, partial benchmark addendum into bench-baseline.json.

The committed top-level `rows` array stays the full-suite baseline (r36). An
addendum only records rows actually measured in a later, narrower window: it
never overwrites the full-suite numbers and never invents missing rows. Each
addendum is recomputed from the raw `megi-bench-v1` pass documents, so it is
reproducible from gitignored artifacts with `--check`.

Usage:
  python3 tools/update_bench_baseline.py --write --id js-r37 \
    --passes target/bench-r37-js-fixed-run1.json ... \
    --commit 76d3be52 --date 2026-11-25 \
    --method "7 consecutive passes; per-row median of the 7 medians" \
    --load-note "load 2.33-5.64 (shared machine)" \
    --artifacts "target/bench-r37-js-fixed-run1..7.json (not committed)"

  python3 tools/update_bench_baseline.py --check ...  # same flags, no write
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import sys
from pathlib import Path
from typing import NoReturn

BASELINE = Path("docs/status/bench-baseline.json")


def fail(message: str) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(2)


def load_documents(paths: list[Path]) -> list[dict]:
    documents: list[dict] = []
    for path in paths:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            fail(f"cannot read {path}: {exc}")
        if not isinstance(data, dict) or data.get("schema") != "megi-bench-v1":
            fail(f"{path} is not a megi-bench-v1 document")
        if not isinstance(data.get("results"), list):
            fail(f"{path} has no results list")
        documents.append(data)
    if not documents:
        fail("no pass documents given")
    return documents


def measured_rows(documents: list[dict]) -> list[dict]:
    collected: dict[str, dict] = {}
    for document in documents:
        for row in document["results"]:
            if not isinstance(row, dict) or not isinstance(row.get("name"), str):
                fail("a result has no string name")
            try:
                median = float(row["median_ms"])
            except (KeyError, TypeError, ValueError):
                fail(f"{row['name']}: non-numeric median_ms")
            if not math.isfinite(median):
                fail(f"{row['name']}: non-finite median_ms")
            samples = row.get("samples_ms")
            if not isinstance(samples, list) or not samples:
                fail(f"{row['name']}: no samples_ms")
            entry = collected.setdefault(
                row["name"],
                {
                    "name": row["name"],
                    "group": row.get("group", "?"),
                    "detail": row.get("detail", ""),
                    "medians": [],
                    "samples": [],
                },
            )
            if entry["group"] != row.get("group") or entry["detail"] != row.get("detail"):
                fail(f"{row['name']}: group/detail changed between passes")
            entry["medians"].append(median)
            for sample in samples:
                try:
                    value = float(sample)
                except (TypeError, ValueError):
                    fail(f"{row['name']}: non-numeric sample {sample!r}")
                if not math.isfinite(value):
                    fail(f"{row['name']}: non-finite sample")
                entry["samples"].append(value)

    rows: list[dict] = []
    for name in sorted(collected):
        entry = collected[name]
        medians = entry["medians"]
        samples = sorted(entry["samples"])
        p95 = samples[max(0, math.ceil(len(samples) * 0.95) - 1)]
        rows.append(
            {
                "name": name,
                "group": entry["group"],
                "median_ms": round(statistics.median(medians), 3),
                "median_range_ms": [round(min(medians), 3), round(max(medians), 3)],
                "p95_samples_ms": round(p95, 3),
                "detail": entry["detail"],
            }
        )
    return rows


def build_addendum(args: argparse.Namespace, documents: list[dict]) -> dict:
    first = documents[0]
    for document in documents[1:]:
        for key in ("commit", "worktree_dirty", "rustc", "cpu", "cores", "iterations", "warmup"):
            if document.get(key) != first.get(key):
                fail(f"metadata {key} differs between passes")
    return {
        "id": args.id,
        "scope": "partial: measured rows only; the top-level rows stay the full-suite baseline",
        "commit": args.commit or first.get("commit"),
        "date": args.date,
        "method": args.method,
        "machine": args.machine or f"{first.get('cpu')}, {first.get('cores')} threads, {first.get('rustc')}",
        "load_note": args.load_note,
        "artifacts": args.artifacts,
        "iterations": first.get("iterations"),
        "warmup": first.get("warmup"),
        "passes": len(documents),
        "rows": measured_rows(documents),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, default=BASELINE)
    parser.add_argument("--id", required=True)
    parser.add_argument("--passes", type=Path, nargs="+", required=True)
    parser.add_argument("--commit")
    parser.add_argument("--date", required=True)
    parser.add_argument("--method", required=True)
    parser.add_argument("--load-note", required=True)
    parser.add_argument("--artifacts", required=True)
    parser.add_argument("--machine")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args()

    try:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read baseline {args.baseline}: {exc}")
    if not isinstance(baseline, dict) or baseline.get("schema") != "megi-bench-baseline-v1":
        fail(f"{args.baseline}: unexpected baseline schema")
    if not isinstance(baseline.get("rows"), list):
        fail(f"{args.baseline}: rows is not a list")

    addendum = build_addendum(args, load_documents(args.passes))
    expected = dict(baseline)
    addenda = dict(baseline.get("addenda", {}))
    addenda[args.id] = addendum
    expected["addenda"] = addenda

    if args.check:
        if baseline == expected:
            print(f"PASS: {args.baseline} addendum '{args.id}' matches {len(args.passes)} passes")
            return 0
        print(f"FAIL: {args.baseline} addendum '{args.id}' is stale or inconsistent")
        return 1

    args.baseline.write_text(
        json.dumps(expected, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"wrote {args.baseline}: addendum '{args.id}' with {len(addendum['rows'])} rows")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
