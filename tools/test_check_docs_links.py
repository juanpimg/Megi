#!/usr/bin/env python3
"""Regression tests for the offline docs link checker (#2290)."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import check_docs_links


class CheckDocsLinksTests(unittest.TestCase):
    def test_links_of_keeps_balanced_parentheses_in_target(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "index.md"
            doc.write_text(
                "[spec](docs/spec_(v2).md)\n[ok](docs/ok.md)\n",
                encoding="utf-8",
            )
            self.assertEqual(
                check_docs_links.links_of(doc),
                ["docs/spec_(v2).md", "docs/ok.md"],
            )

    def test_links_of_control_cases(self) -> None:
        # Titles, images and anchors must keep their current behaviour while the
        # destination regex learns balanced parentheses.
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "index.md"
            doc.write_text(
                "[titled](docs/spec_(v2).md \"Spec v2\")\n"
                "![image](docs/spec_(v2).md)\n"
                "[external](https://example.com/spec_(v2).md)\n"
                "[anchor](#foo)\n",
                encoding="utf-8",
            )
            self.assertEqual(
                check_docs_links.links_of(doc),
                [
                    "docs/spec_(v2).md",
                    "https://example.com/spec_(v2).md",
                    "#foo",
                ],
            )

    def test_headings_of_preserves_unicode_letters(self) -> None:
        # GitHub keeps accented letters in heading anchors: `# Código` yields
        # `#código`, not `#codigo`.
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "a.md"
            doc.write_text("# Código\n", encoding="utf-8")
            self.assertEqual(check_docs_links.headings_of(doc), {"código"})

    def test_check_accepts_unicode_anchor(self) -> None:
        # A link spelled exactly like GitHub's anchor must not be a false
        # positive just because the checker decomposes diacritics.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "docs" / "a.md").write_text("# Código\n", encoding="utf-8")
            (root / "index.md").write_text("[x](docs/a.md#código)\n", encoding="utf-8")
            with (
                mock.patch.object(check_docs_links, "REPO_ROOT", root),
                mock.patch.object(check_docs_links, "ROOT_DOCS", ["index.md"]),
            ):
                self.assertEqual(check_docs_links.check(), [])

    def test_check_reports_no_false_positives_for_balanced_parentheses(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "docs" / "spec_(v2).md").write_text("# Spec\n", encoding="utf-8")
            doc = root / "index.md"
            doc.write_text(
                "# Foo\n\n"
                "[spec](docs/spec_(v2).md)\n"
                "[titled](docs/spec_(v2).md \"Spec v2\")\n"
                "![image](docs/spec_(v2).md)\n"
                "[external](https://example.com/spec_(v2).md)\n"
                "[anchor](#foo)\n",
                encoding="utf-8",
            )
            with (
                mock.patch.object(check_docs_links, "REPO_ROOT", root),
                mock.patch.object(check_docs_links, "ROOT_DOCS", ["index.md"]),
            ):
                self.assertEqual(check_docs_links.check(), [])


if __name__ == "__main__":
    unittest.main()
