#!/usr/bin/env python3
"""Compare a fresh benchmark run against the committed baseline (#963).

`tools/bench.sh --json` prints one `megi-bench-v1` document per pass. This script
reads one or more such documents (or several passes, in which case it takes the
median of medians per row) and diffs every row against
`docs/status/bench-baseline.json`.

Regression rules, deliberately conservative because benchmark runners are noisy
and CI machines are slower than a laptop:

* A row is a **regression** when it grows by more than `--threshold` percent
  *and* by more than `--min-ms` milliseconds. The absolute floor keeps
  microsecond-scale rows (`css/selector-match-simple`, `paint/backdrop-blur-*)`
  from firing on measurement noise.
* Rows at or below the floor still appear in the table, marked `noise`, so a
  real slowdown is visible even when it does not fail the gate.
* Missing rows (renamed/removed bench) and new rows are reported separately and
  never fail the gate by themselves.

Exit status: 0 when no row regressed, 1 when at least one did (so CI fails),
2 on a malformed input document.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path


def load_documents(paths: list[Path]) -> list[dict]:
    documents = []
    for path in paths:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            print(f"error: cannot read {path}: {exc}", file=sys.stderr)
            raise SystemExit(2) from exc
        if data.get("schema") != "megi-bench-v1":
            print(
                f"error: {path} is not a megi-bench-v1 document (schema={data.get('schema')!r})",
                file=sys.stderr,
            )
            raise SystemExit(2)
        documents.append(data)
    if not documents:
        print("error: no benchmark documents given", file=sys.stderr)
        raise SystemExit(2)
    return documents


def per_row_medians(documents: list[dict]) -> dict[str, float]:
    samples: dict[str, list[float]] = {}
    for document in documents:
        for row in document.get("results", []):
            samples.setdefault(row["name"], []).append(float(row["median_ms"]))
    return {name: statistics.median(values) for name, values in samples.items()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("actual", nargs="+", type=Path, help="megi-bench-v1 JSON file(s)")
    parser.add_argument(
        "--baseline",
        type=Path,
        default=Path("docs/status/bench-baseline.json"),
        help="baseline file (default: docs/status/bench-baseline.json)",
    )
    parser.add_argument(
        "--threshold", type=float, default=20.0, help="percent growth that counts as a regression"
    )
    parser.add_argument(
        "--min-ms",
        type=float,
        default=0.5,
        help="absolute growth floor in ms below which growth is treated as noise",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON instead of a table")
    args = parser.parse_args()

    try:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: cannot read baseline {args.baseline}: {exc}", file=sys.stderr)
        return 2
    if baseline.get("schema") != "megi-bench-baseline-v1":
        print("error: unexpected baseline schema", file=sys.stderr)
        return 2

    actual = per_row_medians(load_documents(args.actual))
    base_rows = {row["name"]: float(row["median_ms"]) for row in baseline.get("rows", [])}

    compared, regressions, missing, new = [], [], [], []
    for name, base in sorted(base_rows.items()):
        if name not in actual:
            missing.append(name)
            continue
        got = actual[name]
        delta = got - base
        pct = (delta / base * 100.0) if base > 0 else 0.0
        verdict = "ok"
        if delta > args.min_ms and pct > args.threshold:
            verdict = "REGRESSION"
            regressions.append(name)
        elif delta > args.min_ms:
            verdict = "slower"
        elif delta < -args.min_ms:
            verdict = "faster"
        else:
            verdict = "noise"
        compared.append(
            {
                "name": name,
                "group": next(
                    (row.get("group") for row in baseline["rows"] if row["name"] == name), "?"
                ),
                "baseline_ms": base,
                "actual_ms": got,
                "delta_ms": delta,
                "delta_pct": pct,
                "verdict": verdict,
            }
        )
    new = sorted(set(actual) - set(base_rows))

    if args.json:
        print(
            json.dumps(
                {
                    "baseline_commit": baseline.get("commit"),
                    "threshold_pct": args.threshold,
                    "min_ms": args.min_ms,
                    "regressions": regressions,
                    "missing": missing,
                    "new": new,
                    "rows": compared,
                },
                indent=2,
            )
        )
    else:
        print(
            f"baseline: round {baseline.get('round')} ({baseline.get('commit')}), "
            f"threshold {args.threshold:g}% and >{args.min_ms:g} ms\n"
        )
        width = max((len(row["name"]) for row in compared), default=10)
        for row in compared:
            print(
                f"{row['name']:<{width}}  {row['baseline_ms']:9.3f} → {row['actual_ms']:9.3f} ms  "
                f"{row['delta_pct']:+7.1f} %  {row['verdict']}"
            )
        if new:
            print(f"\nnew rows (not in baseline): {', '.join(new)}")
        if missing:
            print(f"missing rows (bench renamed/removed?): {', '.join(missing)}")
        print()

    if regressions:
        print(f"{len(regressions)} regression(s): {', '.join(regressions)}")
        return 1
    print(f"no regressions above {args.threshold:g} % / {args.min_ms:g} ms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
