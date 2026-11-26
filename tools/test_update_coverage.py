#!/usr/bin/env python3
"""Unit tests for tools/update_coverage.py (capability coverage generator)."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("update_coverage.py")
_spec = importlib.util.spec_from_file_location("update_coverage", MODULE_PATH)
update_coverage = importlib.util.module_from_spec(_spec)
sys.modules["update_coverage"] = update_coverage
_spec.loader.exec_module(update_coverage)

PROGRESS_PATH = Path(__file__).with_name("update_progress.py")
_progress_spec = importlib.util.spec_from_file_location("update_progress", PROGRESS_PATH)
update_progress = importlib.util.module_from_spec(_progress_spec)
sys.modules["update_progress"] = update_progress
_progress_spec.loader.exec_module(update_progress)


def _repo_head() -> str:
    import subprocess

    repo = Path(__file__).resolve().parent.parent
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True, timeout=30
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


GATE_PASS = {
    "status": "PASS",
    "verified_commit": _repo_head(),
    "tokenizer": "7050 / 7050",
    "tree": "3904 / 3904",
    "serializer": "230 / 230",
    "css_syntax": "30 / 30",
}


def cap(cap_id: str, area: str, state: str, evidence: str, name: str = "", issue: int = 0) -> str:
    issue_line = f"issue = {issue}\n" if issue else ""
    return (
        "[[capability]]\n"
        f'id = "{cap_id}"\n'
        f'area = "{area}"\n'
        f'name = "{name or cap_id}"\n'
        'source = "spec"\n'
        f'state = "{state}"\n'
        f'evidence = "{evidence}"\n'
        f"{issue_line}"
    )


class CoverageCountingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "crates/example/tests").mkdir(parents=True)
        (self.root / "crates/example/tests/feature.rs").write_text(
            "#[test]\nfn real_capability_test() { assert!(true); }\n", encoding="utf-8"
        )
        (self.root / "fixtures").mkdir()
        (self.root / "fixtures/golden.html").write_text("<p>ok</p>\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def manifest(self, *entries: str) -> list[dict]:
        import tomllib

        text = "\n".join(entries)
        return tomllib.loads(text).get("capability", [])

    def test_overall_is_implemented_over_known(self) -> None:
        caps = self.manifest(
            cap("A", "F1", "done", "fixture:fixtures/golden.html"),
            cap("B", "F1", "done", "test:crates/example/tests/feature.rs::real_capability_test"),
            cap("C", "F1", "missing", "none"),
            cap("D", "F1", "missing", "none"),
        )
        stats = update_coverage.compute_stats(caps, self.root, GATE_PASS)
        self.assertEqual(stats["overall"]["implemented"], 2)
        self.assertEqual(stats["overall"]["known"], 4)
        self.assertEqual(stats["overall"]["percent"], 50)

    def test_partial_counts_as_missing_and_is_listed(self) -> None:
        caps = self.manifest(
            cap("A", "F1", "done", "fixture:fixtures/golden.html"),
            cap("B", "F1", "partial", "none", "half done"),
        )
        stats = update_coverage.compute_stats(caps, self.root, GATE_PASS)
        self.assertEqual(stats["overall"]["implemented"], 1)
        self.assertEqual(stats["overall"]["partial"], 1)
        self.assertEqual(stats["overall"]["percent"], 50)
        self.assertIn("B", {item["id"] for item in stats["missing"]})

    def test_new_capability_grows_denominator_and_lowers_percent(self) -> None:
        base = [cap("A", "F1", "done", "fixture:fixtures/golden.html"), cap("B", "F1", "missing", "none")]
        before = update_coverage.compute_stats(self.manifest(*base), self.root, GATE_PASS)
        after = update_coverage.compute_stats(
            self.manifest(*base, cap("C", "F1", "missing", "none")), self.root, GATE_PASS
        )
        self.assertEqual(before["overall"]["percent"], 50)
        self.assertEqual(after["overall"]["known"], 3)
        self.assertEqual(after["overall"]["percent"], 33)

    def test_done_requires_verifiable_test_evidence(self) -> None:
        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "test:crates/example/tests/feature.rs::real_capability_test"},
            self.root,
            GATE_PASS,
        )
        self.assertTrue(ok, reason)
        bad, bad_reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "test:crates/example/tests/feature.rs::never_written"},
            self.root,
            GATE_PASS,
        )
        self.assertFalse(bad)
        self.assertIn("fn never_written", bad_reason)

        caps = self.manifest(cap("A", "F1", "done", "test:crates/example/tests/feature.rs::never_written"))
        stats = update_coverage.compute_stats(caps, self.root, GATE_PASS)
        self.assertEqual(stats["overall"]["implemented"], 0)
        self.assertEqual(stats["overall"]["unverified"], ["A"])
        self.assertIn("A", {item["id"] for item in stats["missing"]})

    def test_gate_evidence_requires_suite_and_pass(self) -> None:
        good = {"state": "done", "evidence": "gate:tokenizer"}
        self.assertTrue(update_coverage.verify_evidence(good, self.root, GATE_PASS)[0])
        missing_suite = {"state": "done", "evidence": "gate:not_a_suite"}
        self.assertFalse(update_coverage.verify_evidence(missing_suite, self.root, GATE_PASS)[0])
        failed_gate = dict(GATE_PASS, status="FAIL")
        self.assertFalse(update_coverage.verify_evidence(good, self.root, failed_gate)[0])

    def test_none_evidence_forbidden_when_done(self) -> None:
        bad = {"state": "done", "evidence": "none"}
        ok, reason = update_coverage.verify_evidence(bad, self.root, GATE_PASS)
        self.assertFalse(ok)
        self.assertIn("none", reason)


class CoverageArtifactTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "fixtures").mkdir()
        (self.root / "fixtures/a.html").write_text("ok", encoding="utf-8")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def stats(self) -> dict:
        import tomllib

        text = "\n".join(
            [
                cap("A", "F1", "done", "fixture:fixtures/a.html"),
                cap("B", "F7", "missing", "none", "chrome tapa contenido", issue=1012),
                cap("C", "F7", "partial", "none", "a11y"),
            ]
        )
        caps = tomllib.loads(text).get("capability", [])
        return update_coverage.compute_stats(caps, self.root, GATE_PASS)

    def test_svg_has_area_bars(self) -> None:
        svg = update_coverage.render_svg(self.stats())
        self.assertIn("<svg", svg)
        self.assertIn("F1", svg)
        self.assertIn("F7", svg)
        self.assertIn("33%", svg)
        self.assertIn("1/3", svg)

    def test_html_lists_missing_with_issue_links(self) -> None:
        html = update_coverage.render_html(self.stats(), {"last_updated_by": "tests"})
        self.assertIn("issues/1012", html)
        self.assertIn("F7", html)
        self.assertIn("a11y", html)
        self.assertIn("1/3", html)

    def test_json_has_area_breakdown(self) -> None:
        manifest_path = self.root / "docs/status/coverage.toml"
        manifest_path.parent.mkdir(parents=True)
        manifest_path.write_text(
            "\n".join(
                [
                    cap("A", "F1", "done", "fixture:fixtures/a.html"),
                    cap("B", "F7", "missing", "none"),
                ]
            ),
            encoding="utf-8",
        )
        (self.root / "docs/status/progress.toml").parent.mkdir(parents=True, exist_ok=True)
        (self.root / "docs/status/progress.toml").write_text(
            '[gate]\nstatus = "PASS"\n' + "".join(f'{k} = "{v}"\n' for k, v in GATE_PASS.items() if k != "status"),
            encoding="utf-8",
        )
        artifacts = update_coverage.build_artifacts(self.root)
        import json

        payload = json.loads(artifacts["docs/status/coverage.json"])
        self.assertEqual(payload["areas"][0]["id"], "F1")
        self.assertEqual(payload["areas"][0]["implemented"], 1)
        self.assertIn("docs/status/coverage.svg", artifacts)
        self.assertIn("docs/status/coverage.html", artifacts)

    def test_rules_doc_exists(self) -> None:
        rules = Path(__file__).resolve().parent.parent / "docs/status/COVERAGE.md"
        self.assertTrue(rules.is_file())
        text = rules.read_text(encoding="utf-8")
        self.assertIn("denominador", text)
        self.assertIn("gate:", text)


class ValidatorHardeningTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "crates/example/tests").mkdir(parents=True)
        (self.root / "crates/example/tests/feature.rs").write_text(
            "#[test]\nfn real_capability_test_extended() { assert!(true); }\n", encoding="utf-8"
        )

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _write_status(self) -> None:
        status = self.root / "docs/status"
        status.mkdir(parents=True, exist_ok=True)
        (status / "progress.toml").write_text(
            '[gate]\nstatus = "PASS"\n'
            + "".join(f'{k} = "{v}"\n' for k, v in GATE_PASS.items() if k != "status"),
            encoding="utf-8",
        )

    def test_unknown_state_is_rejected(self) -> None:
        self.assertFalse(
            update_coverage.verify_evidence({"state": "don", "evidence": "none"}, self.root, GATE_PASS)[0]
        )

    def test_python_test_evidence_accepts_real_unittest_method(self) -> None:
        # X-FUZZ-LONG is verified by a Python regression test, not a Rust #[test].
        self.root.joinpath("tools").mkdir(exist_ok=True)
        (self.root / "tools/test_soak.py").write_text(
            "import unittest\n\n"
            "class SoakTests(unittest.TestCase):\n"
            "    def test_rotation_fits(self):\n"
            "        self.assertEqual(1, 1)\n",
            encoding="utf-8",
        )
        ok, reason = update_coverage.verify_evidence(
            {
                "state": "done",
                "evidence": "python_test:tools/test_soak.py::test_rotation_fits",
            },
            self.root,
            GATE_PASS,
        )
        self.assertTrue(ok, reason)

    def test_python_test_evidence_rejects_missing_or_assertion_free_method(self) -> None:
        self.root.joinpath("tools").mkdir(exist_ok=True)
        (self.root / "tools/test_soak.py").write_text(
            "import unittest\n\n"
            "class SoakTests(unittest.TestCase):\n"
            "    def helper(self):\n"
            "        return 1\n",
            encoding="utf-8",
        )
        for evidence in (
            "python_test:tools/test_soak.py::test_missing",
            "python_test:tools/test_soak.py::helper",
        ):
            ok, reason = update_coverage.verify_evidence(
                {"state": "done", "evidence": evidence}, self.root, GATE_PASS
            )
            self.assertFalse(ok, f"{evidence} must not verify a capability: {reason}")

    def test_read_gate_rejects_non_table_gate(self) -> None:
        # triage-r40: `gate = 5` used to crash with `TypeError: 'int' object is not
        # iterable` instead of the clean manifest error emitted by load_manifest.
        progress = self.root / "docs/status/progress.toml"
        progress.parent.mkdir(parents=True, exist_ok=True)
        progress.write_text("gate = 5\n", encoding="utf-8")
        with self.assertRaises(SystemExit) as caught:
            update_coverage.read_gate(self.root)
        self.assertEqual(
            caught.exception.code,
            f"error: [gate] must be a table in {progress}",
        )

    def test_build_fails_on_unknown_state(self) -> None:
        self._write_status()
        (self.root / "docs/status/coverage.toml").write_text(
            "[[capability]]\nid = \"X\"\narea = \"F1\"\nname = \"typo\"\n"
            "source = \"spec\"\nstate = \"don\"\nevidence = \"none\"\n",
            encoding="utf-8",
        )
        with self.assertRaises(SystemExit):
            update_coverage.build_artifacts(self.root)

    def test_fn_evidence_requires_word_boundary(self) -> None:
        ok, _ = update_coverage.verify_evidence(
            {"state": "done", "evidence": "test:crates/example/tests/feature.rs::real_capability_test"},
            self.root,
            GATE_PASS,
        )
        self.assertFalse(ok)

    def test_test_evidence_rejects_comment_and_unannotated_fn(self) -> None:
        # #1391: `verify_evidence` used to accept any `fn <name>` substring, so a commented-out
        # function or a plain helper (no `#[test]`, outside a tests/ dir) could "verify" a done
        # capability. Both must be rejected.
        (self.root / "crates/example/tests/comments.rs").write_text(
            "// fn commented_out_test() {}\nlet s = \"fn string_test() {}\";\n",
            encoding="utf-8",
        )
        (self.root / "crates/example/src").mkdir(parents=True, exist_ok=True)
        (self.root / "crates/example/src/lib.rs").write_text(
            "fn plain_helper() {}\n", encoding="utf-8"
        )
        for evidence in (
            "test:crates/example/tests/comments.rs::commented_out_test",
            "test:crates/example/tests/comments.rs::string_test",
            "test:crates/example/src/lib.rs::plain_helper",
        ):
            ok, reason = update_coverage.verify_evidence(
                {"state": "done", "evidence": evidence}, self.root, GATE_PASS
            )
            self.assertFalse(ok, f"{evidence} must not verify a capability: {reason}")

    def test_test_evidence_in_tests_dir_requires_test_attribute(self) -> None:
        # #1834: a plain helper under tests/ used to verify a `done` capability because
        # the `#[test]` check was skipped for any path containing a `tests` component.
        self.root.joinpath("crates/example/tests/helper.rs").write_text(
            "#[allow(dead_code)]\nfn unannotated_helper() { assert!(true); }\n",
            encoding="utf-8",
        )
        ok, reason = update_coverage.verify_evidence(
            {
                "state": "done",
                "evidence": "test:crates/example/tests/helper.rs::unannotated_helper",
            },
            self.root,
            GATE_PASS,
        )
        self.assertFalse(ok, f"unannotated helper must not verify: {reason}")

    def test_test_evidence_requires_an_assertion(self) -> None:
        # #1834: an annotated but empty test used to verify a capability. Require at
        # least one assertion/expect/unwrap macro in the function body.
        self.root.joinpath("crates/example/tests/empty.rs").write_text(
            "#[test]\nfn empty_test() {}\n", encoding="utf-8"
        )
        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "test:crates/example/tests/empty.rs::empty_test"},
            self.root,
            GATE_PASS,
        )
        self.assertFalse(ok, f"empty test must not verify: {reason}")

    def test_test_evidence_accepts_real_test_in_tests_dir(self) -> None:
        # All 53 real evidences under tests/ must keep verifying after the hardening.
        ok, reason = update_coverage.verify_evidence(
            {
                "state": "done",
                "evidence": "test:crates/example/tests/feature.rs::real_capability_test_extended",
            },
            self.root,
            GATE_PASS,
        )
        self.assertTrue(ok, reason)

    def test_fixture_evidence_rejects_empty_file(self) -> None:
        # #1834: `fixture:` used to accept any path via is_file(), including empty files.
        empty = self.root / "empty.html"
        empty.write_text("", encoding="utf-8")
        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "fixture:empty.html"}, self.root, GATE_PASS
        )
        self.assertFalse(ok, f"empty fixture must not verify: {reason}")

    def test_gate_evidence_rejects_stale_verified_commit(self) -> None:
        # #1392: `verified_commit` must be HEAD (or an ancestor); a copied-forward commit
        # must not keep verifying a capability from an old conformance run.
        good = dict(GATE_PASS, verified_commit="deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "gate:tokenizer"}, self.root, good
        )
        # `self.root` is a temp dir with no git; a bogus commit cannot be an ancestor of any
        # HEAD there, so verification must fail.
        self.assertFalse(ok, f"stale/unknown verified_commit must not verify: {reason}")

    def test_run_gate_fails_when_suite_count_unparseable(self) -> None:
        # #1392: if `test_harness --summary` output lacks a suite count, run_gate must not
        # fall back to the manifest count (that hides a broken gate); status must be FAIL.
        import subprocess

        original = update_progress.run

        def fake_run(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
            return subprocess.CompletedProcess(
                args[1] if len(args) > 1 else [], 0, "no parseable suite output\n", ""
            )

        update_progress.run = fake_run  # type: ignore[assignment]
        try:
            gate = update_progress.run_gate(
                self.root, {"tokenizer": "7050 / 7050", "tree": "3904 / 3904"}
            )
            self.assertEqual(gate["status"], "FAIL")
            self.assertNotIn(gate.get("tokenizer"), ("7050 / 7050",))
        finally:
            update_progress.run = original  # type: ignore[assignment]

    def test_non_done_requires_none_evidence(self) -> None:
        ok, _ = update_coverage.verify_evidence(
            {"state": "partial", "evidence": "test:crates/example/tests/feature.rs::real_capability_test_extended"},
            self.root,
            GATE_PASS,
        )
        self.assertFalse(ok)

    def test_progress_rejects_inconsistent_coverage_percent(self) -> None:
        import json

        status = self.root / "docs/status"
        status.mkdir(parents=True, exist_ok=True)
        (status / "coverage.json").write_text(
            json.dumps({"overall": {"percent": 99, "implemented": 6, "known": 10, "partial": 0}}),
            encoding="utf-8",
        )
        self.assertIsNone(update_progress.read_coverage(self.root))


class ProgressIntegrationTests(unittest.TestCase):
    def test_progress_headline_uses_verified_criteria(self) -> None:
        """The headline is derived from acceptance criteria, not from phase estimates (#2294)."""
        data = {
            "stats": {
                "head": "abc1234",
                "date": "2026-11-17",
                "tests": 10,
                "crates": 1,
                "rust_lines": 100,
                "unsafe_forbidden": 1,
            },
            "gate": {
                "status": "PASS",
                "tokenizer": "1 / 1",
                "tree": "1 / 1",
                "serializer": "1 / 1",
                "css_syntax": "1 / 1",
            },
            "phases": [
                {"id": "M0", "title": "basis",
                 "items": ["a — `gate-a` PASS", "b — `gate-b` PASS"], "pending": ["c"]},
                {"id": "M1", "title": "ui", "items": ["d — `gate-d` PASS"], "pending": ["e"]},
            ],
            "areas": [],
            "next_up": [],
            "rounds": [],
            "last_updated_by": "test",
            "subtitle": "test",
            "coverage": {"percent": 60, "implemented": 6, "known": 10, "partial": 1, "unverified": []},
        }
        # 3 verified / 5 criteria = 60 %, regardless of the (separate) 60 % coverage report.
        markdown = update_progress.render_markdown(data)
        self.assertIn("hitos cerrados **0/2**", markdown)
        self.assertIn("criterios verificados **3/5** (**60%**)", markdown)
        self.assertNotIn("legacy phase average", markdown)
        svg = update_progress.render_progress_svg(data)
        self.assertIn("criterios 3/5 · hitos 0/2", svg)
        self.assertIn("60%", svg)
        self.assertIn("Milestone bars", svg)


class BadgeOwnershipTest(unittest.TestCase):
    """The capability badge and the lcov badge must not share a path (#1403)."""

    def test_badge_writers_use_distinct_paths(self) -> None:
        report_path = Path(__file__).with_name("coverage_report.py")
        spec = importlib.util.spec_from_file_location("coverage_report", report_path)
        report = importlib.util.module_from_spec(spec)
        sys.modules["coverage_report"] = report
        spec.loader.exec_module(report)

        # update_coverage.py writes docs/status/badge-coverage.svg from the capability
        # manifest (CI enforces it byte-for-byte); coverage_report.py must use its own file.
        self.assertNotEqual(
            report.BADGE_PATH.name,
            "badge-coverage.svg",
            "coverage_report.py must not overwrite the capability badge",
        )
        self.assertEqual(report.BADGE_PATH.name, "badge-lcov.svg")


class ProgressGateCommandTest(unittest.TestCase):
    def test_gate_uses_the_renamed_harness_package(self) -> None:
        # The modular rename made the package `megi-test-harness`; the old
        # `test_harness` name makes update_progress.py --check report FAIL.
        root = Path(__file__).resolve().parent.parent
        source = (root / "tools" / "update_progress.py").read_text(encoding="utf-8")
        self.assertIn('"megi-test-harness"', source)
        self.assertNotIn('"-p", "test_harness"', source)


class GateFreshnessTests(unittest.TestCase):
    """#1835: gate: evidence must reject verified_commit when gate-relevant paths changed."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self._git("init")
        self._git("config", "user.email", "test@megi.local")
        self._git("config", "user.name", "Megi Test")
        (self.root / "crates/html/src").mkdir(parents=True)
        (self.root / "crates/html/src/lib.rs").write_text("// html v1\n", encoding="utf-8")
        (self.root / "docs").mkdir(parents=True)
        (self.root / "docs/notes.md").write_text("notes v1\n", encoding="utf-8")
        self._git("add", ".")
        self._git("commit", "-m", "initial")
        self.initial_sha = self._git("rev-parse", "HEAD")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _git(self, *args: str) -> str:
        import subprocess

        res = subprocess.run(
            ["git", *args], cwd=self.root, capture_output=True, text=True, check=True
        )
        return res.stdout.strip()

    def _gate(self, verified_commit: str) -> dict[str, str]:
        return dict(GATE_PASS, verified_commit=verified_commit)

    def test_gate_accepts_ancestor_when_only_non_gate_files_changed(self) -> None:
        # Committing generated status/docs after measuring the gate must keep the stamp valid.
        (self.root / "docs/notes.md").write_text("notes v2\n", encoding="utf-8")
        self._git("add", "docs/notes.md")
        self._git("commit", "-m", "docs only")

        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "gate:tokenizer"},
            self.root,
            self._gate(self.initial_sha),
        )
        self.assertTrue(ok, reason)

    def test_gate_rejects_ancestor_when_gate_relevant_paths_changed(self) -> None:
        for rel_path in (
            "crates/html/src/lib.rs",
            "crates/css/src/lib.rs",
            "crates/test_harness/src/lib.rs",
            "tests/html5lib-tests/tree.dat",
            "tests/wpt/css/syntax.html",
        ):
            with self.subTest(rel_path=rel_path):
                verified = self._git("rev-parse", "HEAD")
                target = self.root / rel_path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(f"// updated {rel_path}\n", encoding="utf-8")
                self._git("add", rel_path)
                self._git("commit", "-m", f"touch {rel_path}")

                ok, reason = update_coverage.verify_evidence(
                    {"state": "done", "evidence": "gate:tokenizer"},
                    self.root,
                    self._gate(verified),
                )
                self.assertFalse(
                    ok,
                    f"gate evidence must reject verified_commit predating {rel_path}: {reason}",
                )

    def test_gate_accepts_head_commit(self) -> None:
        (self.root / "crates/html/src/lib.rs").write_text("// html v2\n", encoding="utf-8")
        self._git("add", "crates/html/src/lib.rs")
        self._git("commit", "-m", "html v2")
        head_sha = self._git("rev-parse", "HEAD")

        ok, reason = update_coverage.verify_evidence(
            {"state": "done", "evidence": "gate:tokenizer"},
            self.root,
            self._gate(head_sha),
        )
        self.assertTrue(ok, reason)

    def test_strip_handles_char_literals_containing_quotes_and_braces(self) -> None:
        source = (
            "fn helper<'a>(_x: &'a str) -> (char, char, char, char) {\n"
            "    ('\"', '{', '\\'', '\\\"')\n"
            "}\n"
            "#[test]\n"
            "fn test_after_char_literal() {\n"
            "    assert!(true);\n"
            "}\n"
        )
        stripped = update_coverage._strip_rust_comments_and_strings(source)
        self.assertIn("fn test_after_char_literal", stripped)
        self.assertIn("assert!(true);", stripped)
        self.assertIn("'a", stripped)

        test_file = self.root / "crates/example/tests/char_literals.rs"
        test_file.parent.mkdir(parents=True, exist_ok=True)
        test_file.write_text(source, encoding="utf-8")
        ok, reason = update_coverage.verify_evidence(
            {
                "state": "done",
                "evidence": "test:crates/example/tests/char_literals.rs::test_after_char_literal",
            },
            self.root,
            GATE_PASS,
        )
        self.assertTrue(ok, reason)


class CoverageReportAndSpecCacheTests(unittest.TestCase):
    def test_coverage_report_malformed_lcov_exits_2_without_traceback(self) -> None:
        import contextlib
        import io

        path = Path(__file__).with_name("coverage_report.py")
        spec = importlib.util.spec_from_file_location("coverage_report", path)
        coverage_report = importlib.util.module_from_spec(spec)
        sys.modules["coverage_report"] = coverage_report
        spec.loader.exec_module(coverage_report)

        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "bad.info"
            bad.write_text("SF:src/lib.rs\nLF:oops\nLH:0\nend_of_record\n", encoding="utf-8")
            old_argv = sys.argv
            sys.argv = ["coverage_report.py", str(bad), "--min", "60"]
            stderr = io.StringIO()
            try:
                with contextlib.redirect_stderr(stderr):
                    rc = coverage_report.main()
            finally:
                sys.argv = old_argv
            self.assertEqual(rc, 2)
            self.assertIn("error:", stderr.getvalue())
            self.assertIn("LF", stderr.getvalue())

    @staticmethod
    def _run_lcov_report(trace: str, *, minimum: str | None = "60") -> tuple[int, str]:
        import subprocess

        root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as tmp:
            tracefile = Path(tmp) / "lcov.info"
            tracefile.write_text(trace, encoding="utf-8")
            argv = [sys.executable, "tools/coverage_report.py", str(tracefile)]
            if minimum is not None:
                argv += ["--min", minimum]
            result = subprocess.run(
                argv, cwd=root, capture_output=True, text=True, timeout=30, check=False
            )
        return result.returncode, result.stdout + result.stderr

    def test_coverage_report_rejects_more_lines_hit_than_found(self) -> None:
        rc, output = self._run_lcov_report(
            "SF:src/lib.rs\nLF:1\nLH:5\nFNF:1\nFNH:3\nend_of_record\n"
        )
        self.assertEqual(rc, 2, f"expected rc=2, got {rc}; output: {output!r}")
        self.assertIn("error:", output)
        self.assertIn("LH", output)

    def test_coverage_report_rejects_more_functions_hit_than_found(self) -> None:
        rc, output = self._run_lcov_report(
            "SF:src/lib.rs\nLF:10\nLH:8\nFNF:1\nFNH:3\nend_of_record\n"
        )
        self.assertEqual(rc, 2, f"expected rc=2, got {rc}; output: {output!r}")
        self.assertIn("error:", output)
        self.assertIn("FNH", output)

    def test_coverage_report_rejects_negative_counter(self) -> None:
        rc, output = self._run_lcov_report(
            "SF:src/lib.rs\nLF:10\nLH:-5\nFNF:1\nFNH:0\nend_of_record\n", minimum=None
        )
        self.assertEqual(rc, 2, f"expected rc=2, got {rc}; output: {output!r}")
        self.assertIn("error:", output)
        self.assertIn("LH", output)

    def test_coverage_report_accepts_consistent_counters(self) -> None:
        rc, output = self._run_lcov_report(
            "SF:src/lib.rs\nLF:10\nLH:7\nFNF:2\nFNH:1\nend_of_record\n"
        )
        self.assertEqual(rc, 0, f"expected rc=0, got {rc}; output: {output!r}")
        self.assertIn("lines 7/10 (70.00 %)", output)

    def test_whatwg_spec_concurrent_force_fetches_do_not_collide(self) -> None:
        import os
        import threading
        from unittest import mock

        path = Path(__file__).with_name("whatwg_spec.py")
        spec = importlib.util.spec_from_file_location("whatwg_spec", path)
        whatwg_spec = importlib.util.module_from_spec(spec)
        sys.modules["whatwg_spec"] = whatwg_spec
        spec.loader.exec_module(whatwg_spec)

        payload = b'{"&amp;": {"characters": "&"}}'

        class FakeResp:
            def __init__(self, data: bytes) -> None:
                self._data = data
                self._sent = False
                self.status = 200

            def read(self, _n: int = -1) -> bytes:
                if self._sent:
                    return b""
                self._sent = True
                return self._data

            def __enter__(self) -> "FakeResp":
                return self

            def __exit__(self, *_args: object) -> None:
                return None

        with tempfile.TemporaryDirectory() as tmp:
            cache_dir = Path(tmp)
            barrier = threading.Barrier(2)
            real_replace = os.replace
            results: list[tuple[str, object]] = []
            lock = threading.Lock()

            def synchronized_replace(src: object, dst: object) -> None:
                barrier.wait(timeout=5)
                real_replace(src, dst)

            def worker() -> None:
                try:
                    cached = whatwg_spec.ensure_cached("entities.json", force=True)
                    with lock:
                        results.append(("ok", str(cached)))
                except Exception as exc:  # noqa: BLE001
                    with lock:
                        results.append(("err", repr(exc)))

            with (
                mock.patch.object(whatwg_spec, "CACHE_DIR", cache_dir),
                mock.patch.object(
                    whatwg_spec.urllib.request,
                    "urlopen",
                    side_effect=lambda *_a, **_kw: FakeResp(payload),
                ),
                mock.patch.object(whatwg_spec.os, "replace", side_effect=synchronized_replace),
            ):
                t1 = threading.Thread(target=worker)
                t2 = threading.Thread(target=worker)
                t1.start()
                t2.start()
                t1.join(timeout=5)
                t2.join(timeout=5)

            self.assertEqual(len(results), 2, results)
            self.assertTrue(
                all(kind == "ok" for kind, _ in results),
                f"concurrent force fetches must both succeed, got {results}",
            )
            self.assertEqual((cache_dir / "entities.json").read_bytes(), payload)


if __name__ == "__main__":
    unittest.main()
