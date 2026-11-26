#!/usr/bin/env python3
"""Unit tests for tools/check_parity_schema.py (Python only)."""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_parity_schema.py")
_spec = importlib.util.spec_from_file_location("check_parity_schema", MODULE_PATH)
checker = importlib.util.module_from_spec(_spec)
sys.modules["check_parity_schema"] = checker
_spec.loader.exec_module(checker)

MANIFEST = {"site": [{"id": "example"}, {"id": "rust"}]}


def entry(**overrides: object) -> dict:
    metrics = {
        "size": [1024, 768],
        "threshold": 16,
        "changed_pixels": 100,
        "changed_pct": 0.92,
        "mean_abs_diff": 1.246,
        "diff_bbox": [0, 0, 10, 10],
    }
    metrics.update(overrides)
    return metrics


class ValidateTests(unittest.TestCase):
    def test_valid_baseline_passes(self) -> None:
        self.assertEqual(
            checker.validate({"example": entry(), "rust": entry()}, MANIFEST), []
        )

    def test_missing_manifest_site_is_reported(self) -> None:
        problems = checker.validate({"example": entry()}, MANIFEST)
        self.assertEqual(problems, ["rust: missing from baseline"])

    def test_unknown_baseline_site_is_reported(self) -> None:
        problems = checker.validate(
            {"example": entry(), "rust": entry(), "extra": entry()}, MANIFEST
        )
        self.assertEqual(problems, ["extra: not present in the real-site manifest"])

    def test_missing_key_and_wrong_type(self) -> None:
        broken = entry()
        del broken["threshold"]
        broken["changed_pct"] = "0.9"
        problems = checker.validate({"example": broken, "rust": entry()}, MANIFEST)
        self.assertIn("example: missing key threshold", problems)
        self.assertIn("example: changed_pct has wrong type str", problems)

    def test_out_of_range_and_oversized_counts(self) -> None:
        broken = entry(changed_pct=101, changed_pixels=10**9)
        problems = checker.validate({"example": broken, "rust": entry()}, MANIFEST)
        self.assertTrue(any("changed_pct out of range" in p for p in problems))
        self.assertTrue(any("exceeds the viewport" in p for p in problems))

    def test_bad_size_and_bbox_shapes(self) -> None:
        broken = entry(size=[1024], diff_bbox=[0, 1, 2])
        problems = checker.validate({"example": broken, "rust": entry()}, MANIFEST)
        self.assertTrue(any("size must be" in p for p in problems))
        self.assertTrue(any("diff_bbox must be" in p for p in problems))

    def test_empty_manifest_is_reported(self) -> None:
        problems = checker.validate({"example": entry()}, {"site": []})
        self.assertEqual(problems[0], "manifest has no [[site]] entries")

    def test_non_object_baseline_is_reported(self) -> None:
        self.assertEqual(checker.validate([], MANIFEST), ["baseline must be a JSON object"])


class CliTests(unittest.TestCase):
    def test_cli_accepts_the_versioned_files(self) -> None:
        root = Path(__file__).resolve().parent.parent
        result = subprocess.run(
            [
                sys.executable,
                str(MODULE_PATH),
                "--baseline",
                str(root / "tests/fixtures/visual-parity/baseline.json"),
                "--manifest",
                str(root / "docs/status/real_sites.toml"),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("schema OK (4 sites)", result.stdout)

    def test_cli_reports_invalid_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline, manifest = root / "b.json", root / "m.toml"
            baseline.write_text(json.dumps({"example": entry()}), encoding="utf-8")
            manifest.write_text('[[site]]\nid = "example"\n[[site]]\nid = "rust"\n', encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--baseline",
                    str(baseline),
                    "--manifest",
                    str(manifest),
                ],
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn("rust: missing from baseline", result.stderr)


if __name__ == "__main__":
    unittest.main()
