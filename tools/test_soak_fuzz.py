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


class SoakRotationTests(unittest.TestCase):
    def test_every_asan_target_is_in_the_soak_rotation(self) -> None:
        # #1833: six ASan-matrix harnesses were missing from TARGETS, so the weekly
        # "rotates through every harness" promise was false.
        bins = set(_fuzz_bins()) - DOCUMENTED_SOAK_EXCLUSIONS
        targets = set(_soak_targets())
        missing = sorted(bins - targets)
        self.assertEqual(missing, [], f"ASan targets missing from soak rotation: {missing}")

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


if __name__ == "__main__":
    unittest.main()
