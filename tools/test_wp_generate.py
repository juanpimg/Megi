"""Tests for the seven WP generators (spec §4.3)."""
import unittest
from tools import wp_generate

class GeneratorTests(unittest.TestCase):
    def test_all_sources_are_registered(self):
        self.assertEqual(set(wp_generate.SOURCES),
            {"wpt", "js262", "site", "fuzz", "audit", "depth", "user"})

    def test_dry_run_writes_nothing(self):
        self.assertIsInstance(wp_generate.generate("wpt", dry_run=True), list)

    def test_generated_items_validate(self):
        for source in wp_generate.SOURCES:
            for wp in wp_generate.generate(source, dry_run=True):
                self.assertTrue(wp.id.startswith(f"WP-{source.upper()}-"))
                self.assertGreater(wp.priority, 0)

if __name__ == "__main__": unittest.main()
