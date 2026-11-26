#!/usr/bin/env python3
"""Check that BOARD.md matches the work-package queue and state (Python only).

The queue holds open packages; ``state.json`` holds the in_progress/review/done
lists, including closed packages that have left the queue. Status is validated
per section, not against the raw queue alone.

Usage:
    python3 tools/check_wp_board.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

BOARD = Path("docs/work-packages/BOARD.md")
QUEUE = Path("docs/work-packages/queue")
STATE = Path("docs/work-packages/state.json")
CLOSED = Path("docs/work-packages/closed.toml")
SECTIONS = ("in_progress", "review", "queued", "done")
MAX_IN_PROGRESS = 10
ITEM = re.compile(r"^- (WP-[A-Za-z0-9._-]+)\s*$")


def parse_board(text: str) -> dict[str, list[str]]:
    """Return the WP ids listed under each board section."""
    sections: dict[str, list[str]] = {name: [] for name in SECTIONS}
    current: str | None = None
    for line in text.splitlines():
        heading = re.match(r"^## ([a-z_]+)\s*$", line)
        if heading:
            current = heading.group(1) if heading.group(1) in sections else None
            continue
        if current:
            match = ITEM.match(line)
            if match:
                sections[current].append(match.group(1))
    return sections


def queue_ids(directory: Path) -> set[str]:
    """Read the ids declared by every queue TOML."""
    ids: set[str] = set()
    for path in sorted(directory.glob("*.toml")):
        with path.open("rb") as handle:
            data = tomllib.load(handle)
        identifier = data.get("id")
        if isinstance(identifier, str) and identifier:
            ids.add(identifier)
    return ids


def load_state(path: Path) -> dict[str, list[str]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    return {name: list(data.get(name, [])) for name in ("in_progress", "review", "done")}


def validate(root: Path) -> list[str]:
    """Return consistency problems between BOARD.md, the queue and the state."""
    board_path, queue_path = root / BOARD, root / QUEUE
    if not board_path.is_file():
        return [f"missing {BOARD}"]
    if not queue_path.is_dir():
        return [f"missing {QUEUE}"]
    sections = parse_board(board_path.read_text(encoding="utf-8"))
    state = load_state(root / STATE) if (root / STATE).is_file() else {key: [] for key in SECTIONS[:3]}
    queued = queue_ids(queue_path)
    busy = set(state["in_progress"]) | set(state["review"]) | set(state["done"])
    problems: list[str] = []
    for column in ("in_progress", "review", "done"):
        listed, declared = set(sections[column]), set(state[column])
        for wp in sorted(declared - listed):
            problems.append(f"{wp}: in state[{column}] but missing from BOARD.md")
        for wp in sorted(listed - declared):
            problems.append(f"{wp}: on BOARD.md[{column}] but not in state")
    expected_queued = queued - busy
    for wp in sorted(expected_queued - set(sections["queued"])):
        problems.append(f"{wp}: in queue but missing from BOARD.md[queued]")
    for wp in sorted(set(sections["queued"]) - expected_queued):
        problems.append(f"{wp}: on BOARD.md[queued] but not an open queue package")
    listed = [wp for ids in sections.values() for wp in ids]
    for wp in set(listed):
        count = listed.count(wp)
        if count > 1:
            problems.append(f"{wp}: listed {count} times")
    if len(state["in_progress"]) > MAX_IN_PROGRESS:
        problems.append(
            f"in_progress has {len(state['in_progress'])} packages (max {MAX_IN_PROGRESS})"
        )
    closed_path = root / CLOSED
    if closed_path.is_file():
        with closed_path.open("rb") as handle:
            closed = set(tomllib.load(handle))
        for wp in sorted(set(state["done"]) - closed - queued):
            problems.append(f"{wp}: done but neither in closed.toml nor in the queue")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    problems = validate(root)
    for problem in problems:
        print(f"WP BOARD: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"work package board OK ({len(queue_ids(root / QUEUE))} open packages)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
