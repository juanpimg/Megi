"""Regression tests for the policy gates in `.github/workflows/ci.yml`.

Covers the cargo-tree capture (#2138), the explicit unittest list (#1402), and
the docs-only classifier / umbrella skip rules (#2405).
"""

import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
CI = ROOT / ".github" / "workflows" / "ci.yml"


def discovered_test_modules(root: Path = ROOT) -> list[str]:
    """Every tools/test_*.py module that is a real unittest module.

    ``test_inspector.py`` is the inspector CLI, not a test (see the comment
    above the explicit list in ci.yml); every other file is a test module.
    """
    return sorted(
        f"tools.{path.stem}"
        for path in (root / "tools").glob("test_*.py")
        if path.stem != "test_inspector"
    )


def registered_ci_test_modules(ci: Path = CI) -> list[str]:
    """Read the explicit ``python3 -m unittest`` module list from the docs job."""
    registered: list[str] = []
    collecting = False
    for raw in ci.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line == "python3 -m unittest":
            collecting = True
            continue
        if not collecting:
            continue
        if not line.startswith("tools.test_"):
            break
        registered.append(line)
    return registered


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

    def test_docs_link_tests_run_in_ci(self):
        # The docs link regression module must be listed in the explicit unittest
        # list (#1402): otherwise a passing local test never protects merges.
        self.assertIn("tools.test_check_docs_links", CI.read_text(encoding="utf-8"))

    def test_wpt_report_tests_run_in_ci(self):
        # A new unittest module is invisible to the explicit list in the docs job
        # (#1402) until it is added there; this keeps the WPT --check regression
        # from passing locally while CI never runs it.
        self.assertIn("tools.test_wpt_report", CI.read_text(encoding="utf-8"))

    def test_bench_baseline_tests_run_in_ci(self):
        # The partial-baseline publisher must be exercised by CI, not only by a
        # local discover run (#1402, M8#1 addendum).
        self.assertIn("tools.test_update_bench_baseline", CI.read_text(encoding="utf-8"))

    def test_window_ui_gate_tests_run_in_ci(self):
        # The visual-parity gate's contract tests must protect merges even though
        # the real `--check` cannot run in CI until Chromium references exist.
        self.assertIn("tools.test_check_window_ui", CI.read_text(encoding="utf-8"))

    def test_every_tools_test_module_is_registered_in_ci(self):
        # Every `tools/test_*.py` module that is a real test (i.e. not the
        # `test_inspector` CLI, excluded in the workflow comment at ci.yml:107)
        # must appear in the explicit `python3 -m unittest` list of the docs
        # job (#1402). A module left out passes locally but never protects a
        # merge. Comparing the whole set makes any future module that is added
        # without registering it fail here.
        discovered = discovered_test_modules()
        self.assertNotEqual(discovered, [], "no tools test modules discovered")
        registered = registered_ci_test_modules()
        self.assertEqual(sorted(registered), discovered)

    def test_ci_test_module_list_has_no_duplicates(self):
        # A duplicated entry hides a missing one: the set comparison in
        # test_every_tools_test_module_is_registered_in_ci would still pass
        # while the run is not the exact discovered set.
        registered = registered_ci_test_modules()
        self.assertEqual(len(registered), len(set(registered)))

    def test_guard_detects_an_unregistered_test_module(self):
        # Behavioural proof of the guard: a new test_*.py that is not in the
        # explicit list must show up in discovery and stay absent from the
        # registered list, so the equality assertion above fails in CI.
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            (root / "tools" / "test_brand_new_module.py").write_text("", encoding="utf-8")
            ci = root / "ci.yml"
            ci.write_text("python3 -m unittest\n  tools.test_existing\n", encoding="utf-8")
            self.assertIn("tools.test_brand_new_module", discovered_test_modules(root))
            self.assertNotIn("tools.test_brand_new_module", registered_ci_test_modules(ci))


RUST_GATED_JOBS = (
    "quality", "conformance", "wpt", "release", "msrv", "minimal",
    "package", "security", "unsafe-audit",
)
UMBRELLA_JOBS = (*RUST_GATED_JOBS, "changes", "docs", "workflow-lint", "secret-scan", "docs-links")


def _dedent_yaml_block(block: str, width: int = 10) -> str:
    """Strip the workflow's step indentation while keeping relative nesting."""
    return "\n".join(line[width:] if line.startswith(" " * width) else line for line in block.splitlines())


def classifier_script() -> str:
    """Extract the exact `changes` job classifier shell from the workflow."""
    lines = CI.read_text(encoding="utf-8").splitlines()
    start = next(i for i, line in enumerate(lines) if line.strip() == "docs_only=true")
    end = next(
        i for i in range(start, len(lines))
        if 'echo "docs_only=$docs_only" >> "$GITHUB_OUTPUT"' in lines[i]
    )
    return _dedent_yaml_block("\n".join(lines[start : end + 1]))


def umbrella_script() -> str:
    """Extract the exact `check` job shell from the workflow."""
    lines = CI.read_text(encoding="utf-8").splitlines()
    start = next(
        i for i, line in enumerate(lines)
        if line.strip() == "- name: Verify all required gates succeeded"
    )
    run_line = next(i for i in range(start, len(lines)) if lines[i].strip() == "run: |")
    body: list[str] = []
    for line in lines[run_line + 1 :]:
        if line.strip() and not line.startswith(" " * 10):
            break
        body.append(line)
    return _dedent_yaml_block("\n".join(body))


def run_bash(script: str, *args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "gate.sh"
        path.write_text("#!/usr/bin/env bash\n" + script + "\n", encoding="utf-8")
        return subprocess.run(
            ["bash", str(path), *args], capture_output=True, text=True, env=env
        )


class DocsOnlyClassifierTests(unittest.TestCase):
    """A .md under tests/ or a generated docs artifact must run the matrix (#2405)."""

    def classify(self, *files: str) -> str:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "github-output"
            env = dict(os.environ, GITHUB_OUTPUT=str(output))
            completed = run_bash(
                'changed="$1"\n' + classifier_script(), "\n".join(files), env=env
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            return output.read_text(encoding="utf-8").strip()

    def test_pure_prose_is_docs_only(self):
        self.assertEqual(self.classify("docs/ARCHITECTURE.md", "README.md"), "docs_only=true")

    def test_markdown_test_input_forces_full_matrix(self):
        for path in (
            "tests/html5lib-tests/README.md",
            "tests/fixtures/render/README.md",
            "crates/image/tests/fixtures/README.md",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.classify(path), "docs_only=false")

    def test_generated_docs_artifacts_force_full_matrix(self):
        for path in (
            "docs/status/wpt.json",
            "docs/audit/coverage.tsv",
            "docs/work-packages/BOARD.md",
            "docs/images/showcase.png",
            "PROGRESS.md",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.classify(path), "docs_only=false")

    def test_mixed_change_forces_full_matrix(self):
        self.assertEqual(
            self.classify("docs/GAPS.md", "docs/status/wpt.json"), "docs_only=false"
        )

    def test_only_rust_gates_are_guarded_by_the_classifier(self):
        text = CI.read_text(encoding="utf-8")
        blocks = re.findall(r"(?ms)^  ([a-z0-9-]+):\n(.*?)(?=^  [a-z0-9-]+:\n|\Z)", text)
        guarded = {
            name for name, body in blocks
            if any(
                line.strip() == "if: needs.changes.outputs.docs_only != 'true'"
                for line in body.splitlines()
                if not line.strip().startswith("#")
            )
        }
        self.assertEqual(guarded, set(RUST_GATED_JOBS))


class UmbrellaCheckTests(unittest.TestCase):
    """A skipped gate is not success unless the classifier expected it (#2405)."""

    def run_check(self, overrides: dict[str, str] | None = None, docs_only: str = "false", omit: tuple[str, ...] = ()):
        results = {name: "success" for name in UMBRELLA_JOBS}
        results.update(overrides or {})
        script = umbrella_script()
        for name in omit:
            del results[name]
            script = script.replace(
                "%s:${{ needs.%s.result }} " % (name, name), ""
            )
        for name, result in results.items():
            script = script.replace("${{ needs.%s.result }}" % name, result)
        script = script.replace("${{ needs.changes.outputs.docs_only }}", docs_only)
        self.assertNotIn("${{", script, "unrendered GitHub expression in umbrella script")
        return run_bash(script)

    def test_all_success_passes(self):
        completed = self.run_check()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("All required gates succeeded", completed.stdout)

    def test_unexpected_skipped_rust_gate_fails(self):
        completed = self.run_check({"quality": "skipped"})
        self.assertEqual(completed.returncode, 1)
        self.assertIn("quality did not succeed: skipped", completed.stderr)

    def test_docs_only_skip_is_expected_and_reported(self):
        completed = self.run_check(
            {name: "skipped" for name in RUST_GATED_JOBS}, docs_only="true"
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("expected docs-only skips:", completed.stdout)
        self.assertIn("unsafe-audit", completed.stdout)

    def test_unconditional_gate_cannot_skip_even_on_docs_only(self):
        for gate in ("changes", "docs", "workflow-lint", "secret-scan", "docs-links"):
            with self.subTest(gate=gate):
                completed = self.run_check({gate: "skipped"}, docs_only="true")
                self.assertEqual(completed.returncode, 1)
                self.assertIn(f"{gate} did not succeed: skipped", completed.stderr)

    def test_conditional_gate_failure_or_cancellation_fails_on_docs_only(self):
        for result in ("failure", "cancelled", "neutral"):
            with self.subTest(result=result):
                completed = self.run_check({"conformance": result}, docs_only="true")
                self.assertEqual(completed.returncode, 1)
                self.assertIn(f"conformance did not succeed: {result}", completed.stderr)

    def test_missing_required_gate_fails(self):
        completed = self.run_check(omit=("security",))
        self.assertEqual(completed.returncode, 1)
        self.assertIn("security missing from umbrella results", completed.stderr)

    def test_umbrella_does_not_treat_skipped_as_generic_success(self):
        self.assertNotRegex(CI.read_text(encoding="utf-8"), r'"success"\] && \[ "\$result" != "skipped"')


if __name__ == "__main__":
    unittest.main()
