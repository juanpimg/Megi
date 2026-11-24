#!/usr/bin/env python3
"""Regression: the weekly soak campaign must rotate through every harness (#1833)."""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOAK = ROOT / "tools" / "soak_fuzz.sh"
FUZZ_CARGO = ROOT / "fuzz" / "Cargo.toml"
SOAK_WORKFLOW = ROOT / ".github" / "workflows" / "fuzz-soak.yml"

# `charset` is compiled but intentionally excluded from the scheduled ASan/UBSan
# campaigns (docs/FUZZING.md: "not currently in the scheduled ASan/UBSan matrix").
DOCUMENTED_SOAK_EXCLUSIONS = {"charset"}


def _soak_targets() -> list[str]:
    text = SOAK.read_text(encoding="utf-8")
    block = re.search(r"TARGETS=\(\n(.*?)\n\)", text, re.S)
    assert block, "soak script must define a TARGETS array"
    return re.findall(r"^\s+([a-z0-9_]+)\s*$", block.group(1), re.M)


def _fuzz_bins() -> list[str]:
    text = FUZZ_CARGO.read_text(encoding="utf-8")
    return re.findall(r'\[\[bin\]\]\s*\nname = "([^"]+)"', text)


def _slice_seconds() -> int:
    text = SOAK.read_text(encoding="utf-8")
    match = re.search(r"^SLICE_SECONDS=(\d+)$", text, re.M)
    assert match, "soak script must define SLICE_SECONDS"
    return int(match.group(1))


def _workflow_default_hours() -> int:
    text = SOAK_WORKFLOW.read_text(encoding="utf-8")
    match = re.search(r'default:\s*["\']?(\d+)["\']?', text)
    assert match, "soak workflow must define a default hours budget"
    return int(match.group(1))


def _workflow_shard_count() -> int:
    text = SOAK_WORKFLOW.read_text(encoding="utf-8")
    match = re.search(r"--shard-count\s+(\d+)", text)
    assert match, "soak workflow must pass --shard-count"
    count = int(match.group(1))
    matrix = re.search(r"^\s*shard:\s*\[(.*?)\]", text, re.M)
    assert matrix, "soak workflow must define a shard matrix"
    indices = [int(v.strip()) for v in matrix.group(1).split(",") if v.strip()]
    assert indices == list(range(count)), f"shard matrix {indices} must cover 0..{count - 1}"
    return count


def _workflow_slice_seconds() -> int:
    text = SOAK_WORKFLOW.read_text(encoding="utf-8")
    # The dispatched input default is the authoritative slice; the run step forwards
    # it as `--slice-seconds "$SLICE_SECONDS"`.
    match = re.search(r"slice_seconds\s*\|\|\s*['\"](\d+)['\"]", text)
    assert match, "soak workflow must define a deeper slice_seconds default"
    assert "--slice-seconds" in text, "soak workflow must forward --slice-seconds"
    return int(match.group(1))


def _shard_targets(targets: list[str], count: int, index: int) -> list[str]:
    """Mirrors the deterministic round-robin shard assignment in soak_fuzz.sh."""
    return [target for i, target in enumerate(targets) if i % count == index]


class SoakRotationTests(unittest.TestCase):
    def test_every_asan_target_is_in_the_soak_rotation(self) -> None:
        # #1833: six ASan-matrix harnesses were missing from TARGETS, so the weekly
        # "rotates through every harness" promise was false.
        bins = set(_fuzz_bins()) - DOCUMENTED_SOAK_EXCLUSIONS
        targets = set(_soak_targets())
        missing = sorted(bins - targets)
        self.assertEqual(missing, [], f"ASan targets missing from soak rotation: {missing}")

    def test_new_fuzz_seed_targets_are_in_the_soak_rotation(self) -> None:
        # 65685d3d added three ASan harnesses but only wired the [[bin]] manifest;
        # without a TARGETS entry the weekly campaign never fuzzes them.
        targets = set(_soak_targets())
        for name in ("cookie_parse", "css_animation", "svg_path"):
            self.assertIn(name, targets, f"{name} must be in TARGETS for the soak campaign")

    def test_one_full_rotation_fits_in_the_default_budget(self) -> None:
        # #1833: 21 targets * 900 s = 18 900 s exceeded the 5 h (18 000 s) workflow
        # budget, so the last target never ran before the job timed out.
        needed = len(_soak_targets()) * _slice_seconds()
        budget = _workflow_default_hours() * 3600
        self.assertLessEqual(
            needed,
            budget,
            f"one rotation needs {needed}s but the workflow only budgets {budget}s",
        )

    def test_js_targets_are_present_so_leak_bracket_is_reachable(self) -> None:
        # #1833: the `-detect_leaks=0` case for js_eval/js_module was dead code
        # because neither target was in TARGETS.
        targets = set(_soak_targets())
        for name in ("js_eval", "js_module"):
            self.assertIn(name, targets, f"{name} must be in TARGETS for the leak flag to apply")

    def test_shards_partition_every_target_exactly_once(self) -> None:
        # X-FUZZ-LONG: the weekly campaign is split across parallel shards so each
        # target gets a deeper run inside the same hosted-runner budget. No target
        # may be dropped or duplicated by the round-robin assignment.
        targets = _soak_targets()
        count = _workflow_shard_count()
        self.assertGreater(count, 1, "the soak campaign must run more than one shard")
        assigned = [t for i in range(count) for t in _shard_targets(targets, count, i)]
        self.assertEqual(sorted(assigned), sorted(targets), "shards must cover every target exactly once")

    def test_deep_sharded_rotation_fits_the_default_budget(self) -> None:
        # With `shard_count` shards, each shard rotates over ceil(n / shards) targets;
        # that subset must still fit in the 5 h budget at the (deeper) workflow slice.
        targets = _soak_targets()
        count = _workflow_shard_count()
        per_shard = (len(targets) + count - 1) // count
        needed = per_shard * _workflow_slice_seconds()
        budget = _workflow_default_hours() * 3600
        self.assertLessEqual(
            needed,
            budget,
            f"a shard needs {needed}s but the workflow only budgets {budget}s",
        )
        # The point of sharding is a deeper per-target campaign than the 600 s default.
        self.assertGreater(
            _workflow_slice_seconds(),
            _slice_seconds(),
            "the sharded campaign must run each target longer than the script default",
        )

    def test_soak_script_accepts_and_validates_shard_args(self) -> None:
        # The script must expose the shard flags and reject invalid values before
        # they reach the shell arithmetic that selects the subset.
        text = SOAK.read_text(encoding="utf-8")
        for token in ("--shard-index", "--shard-count", "SHARD_COUNT=0", "SHARD_INDEX=-1"):
            self.assertIn(token, text, f"soak script must handle {token}")


if __name__ == "__main__":
    unittest.main()
