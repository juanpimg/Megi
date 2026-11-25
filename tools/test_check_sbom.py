#!/usr/bin/env python3
"""Unit tests for tools/check_sbom.py (release SBOM artifact validation)."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout, redirect_stderr
from io import StringIO
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_sbom.py")
_spec = importlib.util.spec_from_file_location("check_sbom", MODULE_PATH)
check_sbom = importlib.util.module_from_spec(_spec)
sys.modules["check_sbom"] = check_sbom
_spec.loader.exec_module(check_sbom)


VALID_BOM = {"bomFormat": "CycloneDX", "specVersion": "1.5", "components": []}


class CheckSbomTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self._dir = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self) -> int:
        with redirect_stdout(StringIO()), redirect_stderr(StringIO()):
            return check_sbom.main([str(self._dir)])

    def test_empty_directory_fails(self) -> None:
        # #1837: an empty sbom-dist must fail the job instead of being
        # published as a "successful" empty artifact.
        self.assertEqual(self._run(), 1)

    def test_invalid_json_fails(self) -> None:
        (self._dir / "broken.json").write_text("{not json", encoding="utf-8")
        self.assertEqual(self._run(), 1)

    def test_non_utf8_bytes_fail_without_traceback(self) -> None:
        # A UTF-16 BOM is not UTF-8 SBOM JSON: reject it cleanly, never crash.
        (self._dir / "sbom.json").write_bytes(b'\xff\xfe{"bomFormat":"CycloneDX"}')
        stderr = StringIO()
        with redirect_stdout(StringIO()), redirect_stderr(stderr):
            rc = check_sbom.main([str(self._dir)])
        self.assertEqual(rc, 1)
        self.assertIn("invalid SBOM JSON", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())

    def test_valid_cyclonedx_json_passes(self) -> None:
        (self._dir / "sbom.json").write_text(json.dumps(VALID_BOM), encoding="utf-8")
        self.assertEqual(self._run(), 0)

    def test_valid_json_among_broken_files_fails(self) -> None:
        (self._dir / "sbom.json").write_text(json.dumps(VALID_BOM), encoding="utf-8")
        (self._dir / "broken.json").write_text("nope", encoding="utf-8")
        self.assertEqual(self._run(), 1)


class ReleaseWorkflowTests(unittest.TestCase):
    """The release workflow must not silently publish an empty SBOM (#1837)."""

    def setUp(self) -> None:
        root = Path(__file__).resolve().parent.parent
        self._text = (root / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")

    def _sbom_step(self) -> str:
        start = self._text.index("      - name: Collect SBOM files")
        end = self._text.index("      - name: Upload SBOM artifacts")
        return self._text[start:end]

    def test_collect_sbom_does_not_swallow_errors(self) -> None:
        step = self._sbom_step()
        self.assertNotIn("|| true", step, "SBOM collection must fail on errors, not swallow them")

    def test_sbom_upload_fails_on_missing_files(self) -> None:
        upload = self._text[self._text.index("      - name: Upload SBOM artifacts"):]
        upload = upload[: upload.index("\n  publish:")]
        self.assertIn("if-no-files-found: error", upload)

    def test_sbom_collect_validates_output(self) -> None:
        self.assertIn("check_sbom.py", self._sbom_step())

    def test_release_create_fallback_is_narrow(self) -> None:
        publish = self._text[self._text.index("  publish:"):]
        self.assertNotIn(
            "2>/dev/null ||",
            publish,
            "release-create stderr must not be discarded before a fallback",
        )
        self.assertIn("already exists", publish, "fallback must be limited to an existing release")


if __name__ == "__main__":
    unittest.main()
