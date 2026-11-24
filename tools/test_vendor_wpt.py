#!/usr/bin/env python3
"""Tests for the SHA-pinned WPT vendor manifest (plan Task 1)."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tools import vendor_wpt

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = (
    "css/css-syntax/declarations-trim-whitespace.html",
    "css/css-syntax/input-preprocessing.html",
    "css/css-syntax/anb-parsing.html",
)
COUNTS = (9, 13, 8)
PIN = "ab2ef95d3a7c512a6e7321688c80e428086af426"


def manifest_bytes(wpt_dir: Path) -> list[dict]:
    entries = []
    for relative, count in zip(FIXTURES, COUNTS):
        data = (wpt_dir / relative).read_bytes()
        entries.append(
            {
                "path": relative,
                "sha256": vendor_wpt.sha256_bytes(data),
                "bytes": len(data),
                "license": "BSD-3-Clause",
                "kind": "testharness",
                "skip_reason": "",
                "dir": relative.rsplit("/", 1)[0],
                "subtest_count_expected": count,
            }
        )
    readme = (wpt_dir / "css/css-syntax/README.md").read_bytes()
    entries.append(
        {
            "path": "css/css-syntax/README.md",
            "sha256": vendor_wpt.sha256_bytes(readme),
            "bytes": len(readme),
            "license": "BSD-3-Clause",
            "kind": "skip",
            "skip_reason": "documentation (not a WPT fixture)",
            "dir": "css/css-syntax",
            "subtest_count_expected": 0,
        }
    )
    return entries


class VendorWptTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.wpt = self.root / "tests" / "wpt"
        self.wpt.mkdir(parents=True)
        for relative in FIXTURES:
            target = self.wpt / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / "tests" / "wpt" / relative).read_bytes())
        (self.wpt / "css/css-syntax/README.md").write_text("fixture README\n", encoding="utf-8")
        self.write({"revision": PIN, "source": "web-platform-tests/wpt", "phase": 1}, manifest_bytes(self.wpt))

    def write(self, revision: dict, entries: list[dict]) -> None:
        (self.wpt / "WPT-REVISION.json").write_text(json.dumps(revision, indent=2) + "\n", encoding="utf-8")
        (self.wpt / "MANIFEST.json").write_text(json.dumps(entries, indent=2) + "\n", encoding="utf-8")

    def test_valid_manifest_is_clean(self) -> None:
        self.assertEqual(vendor_wpt.check_manifest(self.root), [])

    def test_undeclared_file_is_reported_with_path(self) -> None:
        extra = self.wpt / "css/css-syntax/extra.html"
        extra.write_bytes(b"<html></html>\n")
        errors = vendor_wpt.check_manifest(self.root)
        self.assertTrue(any("css/css-syntax/extra.html" in error for error in errors), errors)

    def test_sha_mismatch_is_reported(self) -> None:
        target = self.wpt / FIXTURES[0]
        target.write_bytes(target.read_bytes() + b"<!-- drift -->\n")
        errors = vendor_wpt.check_manifest(self.root)
        self.assertTrue(any("sha256" in error for error in errors), errors)

    def test_unknown_license_is_reported(self) -> None:
        entries = manifest_bytes(self.wpt)
        entries[0]["license"] = "Proprietary"
        self.write({"revision": PIN, "source": "web-platform-tests/wpt", "phase": 1}, entries)
        errors = vendor_wpt.check_manifest(self.root)
        self.assertTrue(any("license" in error for error in errors), errors)

    def test_missing_declared_file_is_reported(self) -> None:
        (self.wpt / FIXTURES[1]).unlink()
        errors = vendor_wpt.check_manifest(self.root)
        self.assertTrue(any(FIXTURES[1] in error for error in errors), errors)

    def test_repository_manifest_is_byte_identical(self) -> None:
        self.assertEqual(vendor_wpt.check_manifest(ROOT), [])
        revision = vendor_wpt.load_revision(ROOT)
        self.assertEqual(revision["revision"], PIN)
        self.assertEqual(
            sum(entry["subtest_count_expected"] for entry in vendor_wpt.load_manifest(ROOT)),
            30,
        )


if __name__ == "__main__":
    unittest.main()
