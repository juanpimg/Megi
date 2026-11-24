"""Regression tests for the dependency-policy gates in `.github/workflows/ci.yml` (#2138)."""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
CI = ROOT / ".github" / "workflows" / "ci.yml"


class CargoTreeGateTests(unittest.TestCase):
    def test_js_free_gate_does_not_mask_cargo_tree_failure(self):
        # The Light WebView build must not use `! cargo tree ... | grep`: without
        # pipefail, a failing `cargo tree` feeds empty input to grep, grep returns 1,
        # and `!` flips the whole pipeline into a green result. The command output
        # must be captured first so `cargo tree`'s own exit status is observed.
        text = CI.read_text(encoding="utf-8")
        # No command line may negate a piped `cargo tree` (comments explaining
        # the old failure mode are fine).
        self.assertNotRegex(text, r"(?m)^\s*!\s*cargo tree\b")
        self.assertIn('tree="$(cargo tree -p megi-webview --no-default-features -e normal)"', text)
        self.assertIn("set -o pipefail", text)
        # The grep that enforces the policy must read the captured output, not a pipe.
        self.assertRegex(
            text,
            r"!.*grep -Eqi 'boa\|winit\|softbuffer'",
        )

    def test_gate_fails_closed_when_cargo_tree_fails(self):
        # Behavioural proof of the bug: the old form under `set -e` returns 0 when
        # the producer fails, the captured form propagates the failure.
        import subprocess

        old = subprocess.run(
            [
                "bash",
                "-c",
                'set -e; ! false 2>/dev/null | grep -Eqi "boa|winit|softbuffer"; echo "gate_rc=$?"',
            ],
            capture_output=True,
            text=True,
        )
        self.assertIn("gate_rc=0", old.stdout)  # documents the failure mode

        fixed = subprocess.run(
            [
                "bash",
                "-c",
                'set -e; out="$(false)" || exit 1; ! printf "%s" "$out" | grep -Eqi "boa|winit|softbuffer"; echo "gate_rc=$?"',
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotIn("gate_rc=0", fixed.stdout)


class ToolsTestListTests(unittest.TestCase):
    def test_audit_matrix_tests_run_in_ci(self):
        # The negative-matrix contract tests (docs/audit/negative-matrix.toml)
        # must be part of the explicit unittest list in the docs job: a module
        # that is not listed never protects merges, however green it is locally.
        self.assertIn("tools.test_audit_negative_matrix", CI.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
