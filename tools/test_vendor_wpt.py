#!/usr/bin/env python3
"""Tests for the SHA-pinned WPT vendor manifest (plan Task 1)."""

from __future__ import annotations

import json
import subprocess
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

    def test_repository_adopts_at_least_100_executable_subtests(self) -> None:
        """M4 floor: the manifest is the denominator; 100+ cases must be really executed."""
        adopted = sum(entry["subtest_count_expected"] for entry in vendor_wpt.load_manifest(ROOT))
        self.assertGreaterEqual(
            adopted,
            100,
            f"manifest adopts only {adopted} subtests; the M4 floor is 100",
        )

    def test_import_files_records_canvas_subset_without_dropping_existing_entries(self) -> None:
        """M6#3: explicit files can be adopted without revendoring whole directories."""
        source = self.root / "wpt-source"
        source.mkdir()
        (source / "LICENSE").write_text("BSD 3-Clause\n", encoding="utf-8")
        fixture = source / "2dcontext/rect/2d.fillRect.basic.html"
        fixture.parent.mkdir(parents=True)
        fixture.write_bytes(b"<title>canvas</title>\n")
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        subprocess.run(["git", "-C", str(source), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(source), "-c", "user.email=test@example.invalid",
             "-c", "user.name=Test", "commit", "-qm", "fixture"],
            check=True,
        )
        subprocess_run = vendor_wpt.import_files(
            source, PIN, ["2dcontext/rect/2d.fillRect.basic.html"], self.root,
            verify_commit=False,
        )
        self.assertEqual(subprocess_run, [])
        entries = vendor_wpt.load_manifest(self.root)
        new = [entry for entry in entries if entry["path"].endswith("2d.fillRect.basic.html")]
        self.assertEqual(len(new), 1)
        self.assertEqual(new[0]["kind"], "testharness")
        self.assertEqual(new[0]["subtest_count_expected"], 1)
        self.assertEqual(
            (self.wpt / "2dcontext/rect/2d.fillRect.basic.html").read_bytes(),
            fixture.read_bytes(),
        )
        self.assertEqual(vendor_wpt.check_manifest(self.root), [])

    def test_import_files_records_webgl_fixture_and_support(self) -> None:
        """M6#4: the WebGL conformance case and its helper are adopted verbatim."""
        source = self.root / "wpt-source-webgl"
        source.mkdir()
        (source / "LICENSE").write_text("BSD 3-Clause\n", encoding="utf-8")
        fixture = source / "webgl/bufferSubData.html"
        fixture.parent.mkdir(parents=True)
        fixture.write_bytes(b"<title>bufferSubData</title>\n")
        helper = source / "webgl/common.js"
        helper.write_bytes(b"function getGl() {}\n")
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        subprocess.run(["git", "-C", str(source), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(source), "-c", "user.email=test@example.invalid",
             "-c", "user.name=Test", "commit", "-qm", "webgl fixture"],
            check=True,
        )
        errors = vendor_wpt.import_files(
            source,
            PIN,
            ["webgl/bufferSubData.html", "webgl/common.js"],
            self.root,
            verify_commit=False,
        )
        self.assertEqual(errors, [])
        entries = {entry["path"]: entry for entry in vendor_wpt.load_manifest(self.root)}
        self.assertEqual(entries["webgl/bufferSubData.html"]["kind"], "testharness")
        self.assertEqual(entries["webgl/bufferSubData.html"]["subtest_count_expected"], 1)
        self.assertEqual(entries["webgl/common.js"]["kind"], "support")
        self.assertEqual(vendor_wpt.check_manifest(self.root), [])


if __name__ == "__main__":
    unittest.main()
