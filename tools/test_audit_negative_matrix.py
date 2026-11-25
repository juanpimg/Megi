#!/usr/bin/env python3
"""Contract tests for the negative-test matrix audit (TDD first)."""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/audit/negative-matrix.toml"
SUITES = ROOT / "crates/browser/tests"
SYSTEMS = ("linux", "macos", "windows")
THREATS = {"T1", "T2", "T3", "T5", "T9", "F1"}
VERIFY_RE = re.compile(
    r"^cargo test -p megi-browser --test (?P<binary>[A-Za-z0-9_]+) "
    r"(?P<test>[A-Za-z0-9_]+) --locked$"
)


class NegativeMatrixContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        from tools import audit_negative_matrix as matrix

        cls.matrix = matrix
        cls.data = tomllib.loads(MATRIX.read_text(encoding="utf-8"))

    def _slots(self, system: str) -> list[dict]:
        return self.data["os"][system]["slots"]

    def test_twelve_unique_ids_in_every_os_section(self) -> None:
        for system in SYSTEMS:
            ids = [slot["id"] for slot in self._slots(system)]
            self.assertEqual(len(ids), 12, f"{system}: expected 12 slots")
            self.assertEqual(len(set(ids)), 12, f"{system}: duplicate ids")
        linux_ids = {slot["id"] for slot in self._slots("linux")}
        for system in SYSTEMS[1:]:
            self.assertEqual({slot["id"] for slot in self._slots(system)}, linux_ids)

    def test_scenarios_are_real_existing_tests_covering_all_threats(self) -> None:
        scenarios = self.matrix.SCENARIOS
        self.assertEqual(len(scenarios), 12)
        self.assertEqual(len({s.id for s in scenarios}), 12)
        self.assertEqual({s.threat for s in scenarios}, THREATS)
        for scenario in scenarios:
            suite = SUITES / f"{scenario.binary}.rs"
            self.assertTrue(suite.is_file(), f"missing suite: {suite}")
            self.assertIn(
                f"fn {scenario.test_name}(",
                suite.read_text(encoding="utf-8"),
                f"invented test: {scenario.binary}::{scenario.test_name}",
            )

    def test_every_verify_cmd_points_to_an_existing_test(self) -> None:
        for system in SYSTEMS:
            for slot in self._slots(system):
                match = VERIFY_RE.match(slot["verify_cmd"])
                self.assertIsNotNone(match, f"{system}/{slot['id']}: bad verify_cmd")
                suite = SUITES / f"{match.group('binary')}.rs"
                self.assertTrue(suite.is_file(), f"missing suite: {suite}")
                self.assertIn(
                    f"fn {match.group('test')}(",
                    suite.read_text(encoding="utf-8"),
                    f"{system}/{slot['id']}: test does not exist",
                )
                self.assertEqual(slot["binary"], match.group("binary"))
                self.assertEqual(slot["test_name"], match.group("test"))

    def test_current_os_passes_with_evidence_others_unsupported(self) -> None:
        current = self.matrix.current_os()
        self.assertEqual(current, "linux")
        self.assertEqual(self.data["current_os"], current)
        self.assertTrue(self.data["generated_at"])
        self.assertTrue(self.data["command_template"])
        self.assertEqual(self.data["runner_env"]["TMPDIR"], "/var/tmp")
        for system in SYSTEMS:
            for slot in self._slots(system):
                if system == current:
                    self.assertEqual(slot["status"], "pass")
                    self.assertEqual(slot["exit_code"], 0)
                    self.assertTrue(slot["verify_cmd"])
                    self.assertTrue(slot["evidence"])
                else:
                    self.assertEqual(slot["status"], "unsupported")
                    self.assertTrue(slot["reason"])

    def test_check_mode_returns_zero_on_generated_state(self) -> None:
        result = subprocess.run(
            [sys.executable, "tools/audit_negative_matrix.py", "--check"],
            cwd=ROOT,
            env=dict(os.environ, TMPDIR="/var/tmp"),
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_check_mode_fails_when_a_slot_is_missing(self) -> None:
        text = MATRIX.read_text(encoding="utf-8")
        marker = "\n[os.macos]\n"
        linux_part, remaining = text.split(marker, 1)
        blocks = linux_part.split("[[os.linux.slots]]")
        self.assertEqual(len(blocks), 13, "unexpected matrix layout")
        broken = blocks[0] + "[[os.linux.slots]]" + "[[os.linux.slots]]".join(blocks[1:-1])
        broken += "[os.macos]\n" + remaining
        with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as handle:
            handle.write(broken)
            path = Path(handle.name)
        try:
            result = subprocess.run(
                [
                    sys.executable,
                    "tools/audit_negative_matrix.py",
                    "--check",
                    "--path",
                    str(path),
                ],
                cwd=ROOT,
                env=dict(os.environ, TMPDIR="/var/tmp"),
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("11", result.stdout + result.stderr)
        finally:
            path.unlink(missing_ok=True)

    def test_check_mode_rejects_malformed_toml_without_traceback(self) -> None:
        with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as handle:
            handle.write("schema = 1\n[os.linux\n")
            path = Path(handle.name)
        try:
            result = subprocess.run(
                [
                    sys.executable,
                    "tools/audit_negative_matrix.py",
                    "--check",
                    "--path",
                    str(path),
                ],
                cwd=ROOT,
                env=dict(os.environ, TMPDIR="/var/tmp"),
                capture_output=True,
                text=True,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 1, output)
            self.assertIn("cannot read matrix", output)
            self.assertNotIn("Traceback", output)
        finally:
            path.unlink(missing_ok=True)

    def _tampered_first_linux_slot(self, field: str, value: str) -> Path:
        text = MATRIX.read_text(encoding="utf-8")
        marker = "\n[os.macos]\n"
        linux_part, remaining = text.split(marker, 1)
        blocks = linux_part.split("[[os.linux.slots]]")
        self.assertEqual(len(blocks), 13, "unexpected matrix layout")
        pattern = re.compile(rf'(?m)^{re.escape(field)} = "[^"]*"$')
        self.assertRegex(blocks[1], pattern, f"no {field} in first linux slot")
        blocks[1] = pattern.sub(f'{field} = "{value}"', blocks[1], count=1)
        tampered = "[[os.linux.slots]]".join(blocks) + marker + remaining
        with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as handle:
            handle.write(tampered)
            return Path(handle.name)

    def _check_tampered_matrix(self, field: str, value: str) -> str:
        path = self._tampered_first_linux_slot(field, value)
        try:
            result = subprocess.run(
                [
                    sys.executable,
                    "tools/audit_negative_matrix.py",
                    "--check",
                    "--path",
                    str(path),
                ],
                cwd=ROOT,
                env=dict(os.environ, TMPDIR="/var/tmp"),
                capture_output=True,
                text=True,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 1, output)
            return output
        finally:
            path.unlink(missing_ok=True)

    def test_check_mode_rejects_pass_without_ok_evidence(self) -> None:
        output = self._check_tampered_matrix("evidence", "warning: stale output")
        self.assertIn("evidence", output)

    def test_check_mode_rejects_threat_drift(self) -> None:
        output = self._check_tampered_matrix("threat", "T9")
        self.assertIn("threat drift", output)

    def test_generator_stays_within_line_budget(self) -> None:
        source = ROOT / "tools/audit_negative_matrix.py"
        self.assertLessEqual(len(source.read_text(encoding="utf-8").splitlines()), 400)


if __name__ == "__main__":
    unittest.main()
