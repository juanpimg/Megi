"""Static checks for the gitleaks allowlists in .gitleaks.toml.

CI runs `gitleaks detect` over the full history with the default ruleset, so a
broad exemption would silently hide a real credential. These tests pin the
single false-positive exemption for docs/status/wpt.json and require it to stay
scoped to one exact path and one rule.
"""

from __future__ import annotations

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONFIG = ROOT / ".gitleaks.toml"
WPT_PATH = r"^docs/status/wpt\.json$"


class GitleaksConfigTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        with CONFIG.open("rb") as handle:
            cls.config = tomllib.load(handle)
        cls.allowlists = cls.config.get("allowlists", [])

    def wpt_allowlists(self) -> list[dict[str, object]]:
        return [
            entry
            for entry in self.allowlists
            if any("wpt.json" in str(path).replace("\\", "") for path in entry.get("paths", []))
        ]

    def test_exactly_one_wpt_exemption(self) -> None:
        self.assertEqual(len(self.wpt_allowlists()), 1, "expected one WPT exemption")

    def test_wpt_exemption_is_path_and_rule_scoped(self) -> None:
        entry = self.wpt_allowlists()[0]
        self.assertEqual(entry.get("paths"), [WPT_PATH])
        self.assertEqual(entry.get("rules"), ["generic-api-key"])
        self.assertNotIn("commits", entry, "commit-wide exemptions are not allowed")
        self.assertNotIn("regexTarget", entry, "regexTarget broadening is not allowed")
        for path in entry.get("paths", []):
            self.assertNotIn(".*", str(path), "path regex must not degrade to .*")

    def test_wpt_exemption_documents_the_reason(self) -> None:
        description = str(self.wpt_allowlists()[0].get("description", "")).lower()
        for word in ("wpt", "canonical", "sha256"):
            self.assertIn(word, description)

    def test_every_allowlist_names_explicit_rules_and_targets(self) -> None:
        for entry in self.allowlists:
            with self.subTest(description=entry.get("description")):
                self.assertTrue(entry.get("rules"), "allowlist must name explicit rules")
                self.assertTrue(
                    entry.get("paths") or entry.get("commits"),
                    "allowlist must name explicit paths or commits",
                )
                for path in entry.get("paths", []):
                    self.assertNotIn(".*", str(path), "path regex must not degrade to .*")


if __name__ == "__main__":
    unittest.main()
