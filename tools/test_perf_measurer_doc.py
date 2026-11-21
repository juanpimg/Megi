"""Source-inspection tests for #1686: the benchmark methodology must document
that layout/* and pipeline/* rows use the deterministic fallback text measurer,
not the production FontTextMeasurer (#1686).

No font stack is needed: these tests only check that the published methodology
states the divergence so the budgets are not read as measuring real shaping.
"""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
PERF_DOC = ROOT / "docs" / "PERFORMANCE.md"


class PerfMeasurerDocumentationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = PERF_DOC.read_text(encoding="utf-8")
        cls.lower = cls.text.lower()

    def test_doc_mentions_issue_1686(self):
        self.assertIn(
            "1686",
            self.text,
            "docs/PERFORMANCE.md must reference #1686 (measurer divergence)",
        )

    def test_doc_names_the_fallback_measurer(self):
        self.assertIn(
            "fallback",
            self.lower,
            "the doc must name the fallback/D deterministic measurer",
        )
        self.assertTrue(
            "measurer" in self.lower or "measurement" in self.lower,
            "the doc must explain the text-measurement path",
        )

    def test_doc_names_the_production_measurer(self):
        self.assertIn(
            "fonttextmeasurer",
            self.lower,
            "the doc must name the production FontTextMeasurer to contrast it",
        )

    def test_doc_scopes_divergence_to_layout_and_pipeline_rows(self):
        self.assertIn(
            "layout/",
            self.lower,
            "the doc must scope the caveat to layout/* rows",
        )
        self.assertIn(
            "pipeline/",
            self.lower,
            "the doc must scope the caveat to pipeline/* rows",
        )


if __name__ == "__main__":
    unittest.main()
