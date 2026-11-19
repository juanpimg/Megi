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


if __name__ == "__main__":
    unittest.main()
