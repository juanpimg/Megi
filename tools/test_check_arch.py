#!/usr/bin/env python3
"""Tests for the architecture linter (Task 0.1 RED fixture)."""

import unittest

try:
    from tools import check_arch
except ImportError:  # RED: the module is implemented in Task 0.2
    check_arch = None


class TestCheckArchPredicates(unittest.TestCase):
    def setUp(self):
        if check_arch is None:
            self.fail("tools.check_arch is missing (Task 0.2 not implemented)")

    def test_unknown_edge_is_violation(self):
        allowed = check_arch.allowed_edges()
        self.assertTrue(
            check_arch.violations_for_edge("paint", "render", allowed),
            "paint -> render must be reported as a forbidden edge",
        )

    def test_known_edge_is_allowed(self):
        allowed = check_arch.allowed_edges()
        self.assertEqual(
            check_arch.violations_for_edge("html", "core", allowed),
            [],
            "html -> core is the allowed direction",
        )

    def test_oversized_file_is_legacy_when_in_baseline(self):
        baseline = {"oversized_files": {"crates/js/src/runtime.rs": 11028}}
        self.assertTrue(
            check_arch.is_legacy("crates/js/src/runtime.rs", baseline),
            "a file recorded in the baseline is legacy, not a new violation",
        )

    def test_forbidden_pattern_catches_rc_static_constructors(self):
        # #1763: the old regex only matched `Rc<`/`Weak<`, so `Rc::new(...)`
        # and friends slipped through the architecture gate.
        pattern = check_arch.FORBIDDEN_RE
        for line in (
            "let host = Rc::new(Host::default());",
            "let weak = Weak::new();",
            "let cloned = Rc::clone(&host);",
            "unsafe { do_thing() }",
        ):
            with self.subTest(line=line):
                self.assertIsNotNone(pattern.search(line), line)

    def test_qualified_paths_are_counted_as_edges(self):
        # #1763: a fully qualified call in an expression is an import edge
        # even though it does not start with `use`.
        edges = check_arch.qualified_edges(
            "let out = megi_render::render_url(url, &opts);", {"render"}
        )
        self.assertEqual(edges, {"render"})

    def test_agents_md_has_hard_rules(self):
        from pathlib import Path

        agents = Path("AGENTS.md")
        self.assertTrue(agents.exists(), "AGENTS.md must exist at the repo root")
        text = agents.read_text()
        for rule in ("400 líneas", "forbid(unsafe_code)", "TDD", "DAG"):
            self.assertIn(rule, text, f"AGENTS.md is missing the rule {rule!r}")
        self.assertLessEqual(len(text.splitlines()), 200, "AGENTS.md must stay under 200 lines")

    def test_workspace_lints_are_inherited(self):
        import tomllib
        from pathlib import Path

        root = tomllib.loads(Path("Cargo.toml").read_text())
        lint = root.get("workspace", {}).get("lints", {}).get("rust", {})
        self.assertEqual(lint.get("unsafe_code"), "forbid")
        for member in root["workspace"]["members"]:
            manifest = tomllib.loads((Path(member) / "Cargo.toml").read_text())
            self.assertEqual(
                manifest.get("lints", {}).get("workspace"), True,
                f"{member} must inherit workspace lints",
            )


    def test_dag_manifest_is_the_single_source_of_truth(self):
        # #1838: the linter must read the DAG from one documented manifest,
        # not from a hand-maintained copy that can silently drift.
        import tomllib
        from pathlib import Path

        manifest_path = Path("docs/architecture-deps.toml")
        self.assertTrue(
            manifest_path.exists(),
            "docs/architecture-deps.toml must be the single source of the crate DAG",
        )
        manifest = tomllib.loads(manifest_path.read_text())
        self.assertIn("edges", manifest)
        self.assertEqual(
            check_arch.allowed_edges(),
            {name: set(deps) for name, deps in manifest["edges"].items()},
            "allowed_edges() must be derived from docs/architecture-deps.toml",
        )

    def test_manifest_core_consumers_match_agents_md(self):
        # #1838: AGENTS.md §3 declares which crates may import `core`.
        import re
        from pathlib import Path

        text = Path("AGENTS.md").read_text()
        match = re.search(r"^\s*core\s*→\s*([a-z,\s]+)$", text, re.MULTILINE)
        self.assertIsNotNone(match, "AGENTS.md must declare the core → ... DAG line")
        declared = {part.strip() for part in match.group(1).split(",") if part.strip()}
        allowed = check_arch.allowed_edges()
        for consumer in sorted(declared):
            with self.subTest(consumer=consumer):
                self.assertIn(
                    "core",
                    allowed.get(consumer, set()),
                    f"AGENTS.md allows {consumer} → core, so the DAG must allow it too",
                )

    def test_net_image_and_fonts_may_import_core(self):
        # #1838: these three were flagged as forbidden edges despite AGENTS.md
        # explicitly listing them as consumers of `core`.
        allowed = check_arch.allowed_edges()
        for consumer in ("net", "image", "fonts"):
            with self.subTest(consumer=consumer):
                self.assertEqual(
                    check_arch.violations_for_edge(consumer, "core", allowed),
                    [],
                    f"{consumer} → core is a legitimate edge per AGENTS.md §3",
                )

    def test_retired_shell_alias_is_gone(self):
        # #1838: `shell` was renamed to `browser`; keeping the alias in the
        # DAG makes the gate enforce a crate that no longer exists.
        self.assertNotIn("shell", check_arch.allowed_edges())

    def test_dev_sh_all_includes_coverage_and_audit_gates(self):
        # #2205: `dev.sh all` must run the same coverage and audit checks as `dev.sh lint`.
        from pathlib import Path

        text = Path("tools/dev.sh").read_text(encoding="utf-8")
        lint_section = text[text.index("    lint)") : text.index("    doc)")]
        all_section = text[text.index("    all)") : text.index("    -h|--help)")]
        for required in (
            "update_progress.py --check --no-gate --no-count",
            "update_coverage.py --check",
            "audit_coverage.py --check",
        ):
            with self.subTest(gate=required):
                self.assertIn(required, lint_section)
                self.assertIn(required, all_section)


class TestAliasedCrateImports(unittest.TestCase):
    """Aliased `use` statements must not bypass the architecture gate."""

    def _scan_with_alias(self, use_line: str) -> tuple[list[str], int]:
        import json
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "docs").mkdir()
            (root / "docs" / "architecture-deps.toml").write_text(
                '[edges]\nhtml = ["core"]\nrender = []\n'
            )
            (root / "docs" / "architecture-baseline.json").write_text(
                json.dumps({"edges": {}, "oversized_files": {}, "forbidden_patterns": {}})
            )
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/html", "crates/render"]\n'
            )
            for member, package in (("html", "megi-html"), ("render", "megi-render")):
                crate = root / "crates" / member
                (crate / "src").mkdir(parents=True)
                (crate / "Cargo.toml").write_text(f'[package]\nname = "{package}"\n')
                (crate / "src" / "lib.rs").write_text("// crate\n")
            (root / "crates" / "html" / "src" / "lib.rs").write_text(use_line + "\n")
            old_root, old_baseline = check_arch.ROOT, check_arch.BASELINE_PATH
            try:
                check_arch.ROOT = root
                check_arch.BASELINE_PATH = root / "docs" / "architecture-baseline.json"
                return check_arch.scan()
            finally:
                check_arch.ROOT, check_arch.BASELINE_PATH = old_root, old_baseline

    def test_aliased_use_is_counted_as_forbidden_edge(self):
        violations, _ = self._scan_with_alias("use megi_render as r;")
        self.assertTrue(
            any("forbidden edge: html -> render" in v for v in violations),
            f"aliased import not reported: {violations}",
        )

    def test_pub_crate_aliased_use_is_counted_as_forbidden_edge(self):
        violations, _ = self._scan_with_alias("pub(crate) use megi_render as r;")
        self.assertTrue(
            any("forbidden edge: html -> render" in v for v in violations),
            f"pub(crate) aliased import not reported: {violations}",
        )


class TestOversizedTestFiles(unittest.TestCase):
    """#2253: AGENTS.md §4 caps every .rs file, including crates/*/tests/."""

    def test_scan_includes_tests_directory_for_size(self):
        # A synthetic oversized test file must be reported, proving the linter
        # scans crates/*/tests/ and not only crates/*/src/.
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            (root / "docs").mkdir()
            (root / "docs" / "architecture-deps.toml").write_text('[edges]\n')
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/dummy"]\n')
            crate = root / "crates" / "dummy"
            (crate / "src").mkdir(parents=True)
            (crate / "tests").mkdir()
            (crate / "Cargo.toml").write_text('[package]\nname = "megi-dummy"\n')
            (crate / "src" / "lib.rs").write_text("// src\n")
            (crate / "tests" / "big.rs").write_text("fn x() {}\n" * 501)
            old_root = check_arch.ROOT
            try:
                check_arch.ROOT = root
                violations, _ = check_arch.scan()
            finally:
                check_arch.ROOT = old_root
        self.assertTrue(
            any("crates/dummy/tests/big.rs" in v and "lines (max" in v for v in violations),
            f"oversized test file not reported: {violations}",
        )


if __name__ == "__main__":
    unittest.main()
