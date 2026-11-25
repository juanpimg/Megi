"""Verify that a milestone acceptance criterion actually closed.

Two selection modes are supported:

* Positional (default): ``python3 tools/check_milestone.py M0 4`` checks
  criterion 4, counting ``items`` first and then ``pending``. This mode is
  unstable when a criterion is closed out of order: moving the last pending
  criterion into ``items`` renumbers the remaining pending criteria.
* Text match: ``python3 tools/check_milestone.py M9 --match "Distribución multiplataforma firmada"``
  searches the criterion text in ``items`` (PASS) or ``pending`` (FAIL),
  case-insensitively and by substring. Use this mode for criteria that can be
  closed out of order; the text is the stable identifier.

Exit 0 only when the selected criterion is no longer pending in
docs/status/progress.toml and has been recorded as verified with evidence.

Usage: python3 tools/check_milestone.py M0 4
       python3 tools/check_milestone.py M9 --match "Distribución multiplataforma firmada"
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

DEFAULT_ROOT = Path(__file__).resolve().parent.parent


def check(
    milestone: str,
    index: int | None,
    root: Path | None = None,
    match: str | None = None,
) -> int:
    root = root or DEFAULT_ROOT
    path = root / "docs/status/progress.toml"
    if not path.is_file():
        print(f"FAIL: {path} not found")
        return 1
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    phase = next((p for p in data.get("phase", []) if str(p.get("id", "")).upper() == milestone.upper()), None)
    if phase is None:
        print(f"FAIL: milestone {milestone} not found")
        return 1
    pending = phase.get("pending", [])
    items = phase.get("items", [])

    # Stable text mode: search both lists independently of their length, so a
    # criterion closed out of order keeps its verification command usable.
    if match is not None:
        needle = match.strip()
        if not needle:
            print(f"FAIL: {milestone} --match has no text")
            return 1
        folded = needle.casefold()
        for evidence in items:
            if isinstance(evidence, str) and folded in evidence.casefold():
                print(f"PASS: {milestone} --match {needle!r} verified: {evidence[:140]}")
                return 0
        for evidence in pending:
            if isinstance(evidence, str) and folded in evidence.casefold():
                print(f"FAIL: {milestone} --match {needle!r} still pending: {evidence[:140]}")
                return 1
        print(f"FAIL: {milestone} --match {needle!r} does not match any criterion")
        return 1

    if index is None:
        print(f"FAIL: {milestone} needs a criterion index or --match TEXT")
        return 1
    if index < 1 or index > len(pending) + len(items):
        print(f"FAIL: criterion {milestone}#{index} does not exist")
        return 1
    # A criterion is closed when it has moved out of `pending` into `items`.
    if index <= len(items):
        evidence = items[index - 1]
        if not isinstance(evidence, str) or not evidence.strip():
            print(f"FAIL: {milestone}#{index} has no recorded evidence")
            return 1
        print(f"PASS: {milestone}#{index} verified: {evidence[:140]}")
        return 0
    pending_index = index - len(items)
    if pending_index <= len(pending):
        evidence = pending[pending_index - 1]
        if not isinstance(evidence, str) or not evidence.strip():
            print(f"FAIL: {milestone}#{index} has no recorded evidence")
            return 1
        print(f"FAIL: {milestone}#{index} still pending: {evidence[:140]}")
    else:
        print(f"FAIL: {milestone}#{index} has no recorded evidence")
    return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check one milestone acceptance criterion (by index or --match TEXT)"
    )
    parser.add_argument("milestone")
    parser.add_argument("index", type=int, nargs="?", default=None)
    parser.add_argument(
        "--match",
        metavar="TEXT",
        default=None,
        help="select the criterion by stable text instead of its position",
    )
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = parser.parse_args(argv)
    if (args.index is None) == (args.match is None):
        parser.error("provide exactly one of criterion index or --match TEXT")
    return check(args.milestone, args.index, root=args.root, match=args.match)


if __name__ == "__main__":
    raise SystemExit(main())
